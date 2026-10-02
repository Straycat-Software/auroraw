# Design note 005: the image engine's interfaces

> **Status: decided, with amendments, in [D-140, D-141 and D-142](../decisions.md) (2026-09-30).** The
> decisions are the authority; this note is the record of the reasoning. It was a proposal, accepted
> with amendments in [#34](https://github.com/Straycat-Software/auroraw/issues/34): **D-140** is the
> recipe, the render API and the dependency edges, **D-141** the `RawImage` ABI, **D-142** the declaration
> and its parameter types. Written by Charlie (an AI assistant, Claude Code), who does image
> processing and the GPU. It follows Alice's introduction note of 2026-09-30 (§5, items 1 to 7) and
> covers the three seams that had to be agreed before any side coded against them: the **render API**
> (§2), **`RawImage`** (§3) and the **operation declaration** (§5). The other four questions (§4,
> §6 to §8) are positions, with what is still to be measured before they can be more. Alice's
> amendments are folded in and marked *(Alice)*.
>
> Everything here is tagged **[measured]** (a number, from spike 1 or from a run on the reference
> machine, said which), **[read]** (a fact read in the code or in `rawler`) or **[proposed]**.
> Nothing here has been built.

## 1. Where we start

**What spike 1 built** [read: tag `spikes-final`, `spikes/gpu-pipeline`]: about 700 lines of Rust and
WGSL. Three compute passes wired in a fixed order (demosaic, downscale, tone), a heavier chain of
four more (non-local means, box blurs, a combine pass), one shared **uniform block** (`Params`) that
every shader reads and that carries the parameters of every operation, a CPU twin of the maths,
and three paths (full export by bands, a 100 % view, a fit-to-screen view). It is a good proof
and a poor skeleton, and this is what the product must change:

| In the spike | In the product |
| --- | --- |
| Passes and their order are code | A stage graph built from a recipe (§2.2) |
| One `Params` block for everything, edited field by field | One typed parameter block per operation, laid out from its declaration (§5) |
| A uniform buffer allocated for each pass | A pool of parameter buffers, reused |
| Only the 8-bit result leaves the GPU | 16-bit (and float) results too, for 16- and 32-bit TIFF (spec §6, §5.8) [read: `tone.wgsl` packs 8-bit] |
| Averaged black and white levels; one Bayer phase trick; `Scene` built straight from `rawler` | A `RawImage` that carries what the pipeline needs (§3) |
| Adapter chosen by a substring, one device, no recovery | Device loss and out-of-memory as ordinary events (architecture §6.3, §6.6) |

**The reference machine here** [measured, 2026-09-30]: the spike rebuilt from the tag, on the
GTX 1650 SUPER (Vulkan, driver 580.178) and llvmpipe. Its smoke test passes (Bayer, X-Trans,
tiled, the heavy chain, fit-to-screen: at most 1 level, 0 % beyond). The synthetic 24 MP run gives
a full render in 31 ms (27 ms in the report), a tone-only change in 0.57 ms and a 100 % view
recompute in 1.4 ms, so the report reproduces. This GPU is shared with a desktop and other jobs
(2 of 4 GiB in use and 37 % busy during the run): the numbers are indicative, which is one more
reason for `gpu-check` to record what else was running.

## 2. The render API (item 1)

### 2.1 Shape [proposed]

The pipeline is a **headless service**: no catalogue, no sidecar, no Qt (architecture §3.2, rule 3).
It receives plain data and returns pixels and a report.

```text
Pipeline::open(config, registry) -> Result<Pipeline, OpenError>   // starts the GPU thread, picks the adapter
Pipeline::render(RenderRequest) -> RenderHandle                   // returns at once
RenderHandle: wait() / try_recv() / cancel()                      // channels, no async runtime (architecture §3.2, rule 7)

RenderRequest {
    image:    ImageKey + Arc<RawImage>,   // what to develop; the key names the caches (below)
    recipe:   Recipe,                     // §2.2
    view:     View,                       // §2.3
    quality:  Quality,                    // Draft | Final
    output:   OutputTransform,            // §2.4
    priority: Priority,                   // Interactive | Visible | Background (architecture §4.3)
}

Rendered { pixels, width, height, report: RenderReport }
```

- **`ImageKey` = `(PhotoId, fingerprint of the original)`** *(Alice)*, so that the caches drop when
  the file changes under us (D-019, risk 4). It is not called `SourceId`: that name is the storage
  source's, in `types`.
- **The pipeline never loads plugins** *(Alice)*. `open` receives an **`OperationRegistry`** that the
  engine fills from the built-in operations and from `plugin-host`. That keeps `pipeline` off
  `plugin-host`, as the diagram of architecture §3.1 says.
- **`open` returns a typed error when there is no adapter at all** *(Alice)* (a virtual machine, some
  remote sessions), and the adapter can be overridden by a setting (architecture §6.6).

`render` never blocks its caller. Cancellation is a token checked **between passes and between
bands**: a submitted GPU pass cannot be interrupted, so the granularity is one dispatch, and a
pass is sized so that it stays short (§6). For interactive requests the rule is **latest wins**: a
new request for the same image and view supersedes a queued one, so a fast drag does not queue
up stale frames.

### 2.2 What a "version" is to the pipeline [proposed, as Alice wishes]

The pipeline knows nothing of versions, sidecars or history. It receives a **`Recipe`**, a plain
value that `develop` builds from a version:

```text
Recipe {
    definition:  PipelineDefinitionVersion,   // the stage list and its order, versioned (spec §5.6)
    operations:  Vec<OperationInstance>,      // in pipeline order, already placed by `develop`
}
OperationInstance {
    operation:   OperationId,                 // e.g. "auroraw.exposure" or a plugin's identifier
    op_version:  u32,                         // the operation's own version (architecture §7.2)
    enabled:     bool,
    params:      Vec<ParamValue>,             // typed by the operation's declaration, §5
}
```

- **Where the types live** *(Alice)*. `Recipe` and `OperationInstance` live in **`pipeline`**. The
  parameter types (`ParamSpec`, `ParamValue`) and **`OperationId`** live in **`plugin-api`**, because
  plugins declare them: they are part of the public surface (MIT OR Apache-2.0, no dependency on the
  application). They must cover more than scalars: **bool, int, float, enum, colour, point, list and
  a curve** (control points), since the tone curve is an M2 operation and sidecars store the values.
- **No `mask` field until M3** *(Alice)*. An undefined type in a hashed, sidecar-bound struct is a dead
  field somebody will start to depend on. **The definition version is the extension point**: M3 adds
  the field with a new version.
- The base look (D-042) is just a `Recipe` (a style applied to the default version), so an unedited
  photo renders through the same path.
- **The hash is over a canonical binary encoding of the typed values** *(Alice)*: the bit patterns,
  `-0.0` normalised, `NaN` refused, not serialised JSON. The function is **`blake3`**, already in the
  workspace. `hash(recipe[..n])` is the cache key of stage `n` (§2.6) and what `develop` stores next to
  a render as proof of determinism.
- An operation whose plugin is missing arrives as `enabled: false` with its parameters kept
  (architecture §7.2); the pipeline skips it and says so in the report.
- Who **places** an operation (from its declaration's stage and constraints) is `develop`, not the
  pipeline: the pipeline executes the order it is given and only **validates** it (an operation
  outside its stage is refused, and `develop` refuses an unknown stage identifier). This keeps the
  placement rules in one place, and the pipeline testable with hand-written recipes.
- **A new dependency edge, `pipeline` to `plugin-api`** *(Alice)*, for `RawImage` and the parameter
  types. `plugin-api` is a leaf and permissive, so there is no cycle and no licence problem. It goes
  in architecture §3.1 and in the `ALLOWED` table of `xtask`; the edge to `imaging` stays.

### 2.3 Views and output [proposed]

`View` is one of: `Fit { max_width, max_height }`, `Region { rect, scale }` (100 % and zoomed views,
the rectangle in output pixels), or `Full` (export). The pipeline picks the working geometry
(reduced image with scaled radii for `Fit`, halo for `Region`, bands for `Full`) as the spike did
[measured: fit 19 ms against 120 ms for a denoise change]. Coordinates are in the **oriented,
cropped** image, so the caller never handles the sensor's geometry.

**Export must not return one `Vec`.** A 60 MP result is 240 MB in RGBA8 and 480 MB in 16-bit.
`Full` renders **stream bands into a sink** (`PixelSink::write_band(rows, &[u8])`), so an encoder
can start on the first band and memory stays bounded. This is also what makes export work under
memory pressure (§6). **A slow sink must back-pressure the GPU** *(Alice)*: that is what bounds memory.
`Fit` and `Region` sizes are in **device pixels** *(Alice)* (the interface passes physical ones).

### 2.4 Output transform [proposed; colour choices are §7]

`OutputTransform` is `Display { profile }` for what is shown, or `Profile { icc, bit_depth }` for
export. The last stage applies it; the caller never touches colours (D-069, architecture §6.5).
The output pixel format is **RGBA8 for views** (§6.7, kept) and **RGBA16 or float for export**,
which the spike never produced.

### 2.5 The report [proposed]

`RenderReport` carries what the tests and the benchmark need, and nothing the interface must
interpret: the **stages rerun and stages served from cache** (as counts, so a test can assert
"a late change reruns nothing earlier", testing strategy §4.6), timings per stage (for
`gpu-check`, never gated in CI), the adapter and back end, the **quality actually rendered** (a
draft may fall back further), the bands used, whether memory pressure shrank the work, whether
the CPU path was used, and warnings (disabled operations, a stage that fell back).

### 2.6 Stage caches [proposed, from spike 1]

The output of stage `n` is cached for the region and quality being viewed, keyed by
`(image key, hash(recipe[..=n]), geometry, quality)`. A change reruns its stage and every later
one from the cache of the one before [measured: 0.6 ms for a tone change, 1.2 ms for an upstream
one]. The cache is owned by the GPU thread and **budgeted**: evicted first from the earliest
stages of views that are no longer on screen, whole on device loss (§6).

## 3. `RawImage` and the input stages (item 2)

### 3.1 What is missing today [read]

`auroraw-plugin-api::RawImage` (and the `rawler-decoder` plugin's 7-word header behind it) returns
width, height, samples per pixel, `Vec<u16>`, and **one averaged black level and one averaged
white level**. Spike 1's `raw.rs` needed, from `rawler`, everything below the line:

| Field | In `RawImage` today | Needed by |
| --- | --- | --- |
| Samples (`u16`, or `f32` for some DNGs: `RawImageData::Float`) | `u16` only. The plugin **refuses** float data (returns `-2`) | linear DNG (a gap, architecture §6.4) |
| The colour filter pattern (2x2 Bayer, 6x6 X-Trans, others) | no | demosaic, X-Trans pass |
| Black level, per channel or per position of a repeating pattern | averaged | correct blacks (architecture §6.4: "the spike averaged them"); harmless on our eight samples, see §3.3 |
| White level, per component | averaged | same |
| As-shot white balance (`wb_coeffs`, RGBE) | no | white balance |
| Camera colour matrices, per illuminant (`color_matrix`), with a legacy `xyz_to_cam` | no | camera to working space |
| The recommended crop (`crop_area`) and active area | no | geometry |
| Orientation | no | upright output |
| Make and model | no | matrix fallback tables, diagnostics |
| Sensor masked areas (`blackareas`) | no | black level estimation on some cameras, later |
| Fuji rotated sensors (`fuji_rotation_width`) | no | later, not M2 |

### 3.2 Proposal [proposed, accepted with amendments]

Keep the crate's rule (`plugin-api` depends on no other crate of the application and **not on
`rawler`**): the fields are the API's own types, mapped by the decoder plugin. The name `RawImage`
stays for now (experimental until M5); if it reads badly once inputs that are not sensor data
arrive, renaming is cheap *(Alice)*.

```text
RawImage {
    width, height,                      // of the full sensor readout
    layout:      SensorLayout,          // Cfa { width, height, colours: Vec<u8> } | LinearRgb { components, profile } | Mono
    samples:     Samples,               // #[non_exhaustive]: U16(Vec<u16>) now, F32(Vec<f32>) reserved
    levels:      Levels,                // black and white, see below
    white_balance: Option<[f32; 3]>,    // as shot, R G B (rawler's fourth value is NaN on every sample)
    colour:      Vec<ColourMatrix>,     // (illuminant, 3x3 XYZ -> camera); possibly empty
    crop:        Option<Rect>,          // recommended, in sensor pixels; the pattern is anchored at (0, 0), not at the crop
    active_area: Option<Rect>,
    orientation: Orientation,           // the EXIF eight
    camera:      CameraId { make, model },
}
Levels {
    black: BlackLevel { rows, columns, per_component: Vec<f32> },   // a repeating pattern, like rawler's
    white: Vec<f32>,                                                  // per component
}
InputProfile = Icc(bytes) | Named(Srgb | AdobeRgb | Rec2020 | ProPhoto)
```

Reasons and choices, with what Alice's review changed:

1. **Black and white levels keep `rawler`'s model**: a repeating pattern for the black, one value per
   component for the white. The averaging is harmless on thirteen of the fourteen files that decoded
   (§3.3, §3.3b), but **the ABI is the part that is hard to change later, the internal use is not**
   *(Alice)*, so the pattern is in the ABI.
2. **`Samples` is `#[non_exhaustive]` and `F32` is reserved** *(Alice)*. The plugin **keeps refusing
   float data until a test on a CC0 float file exists**, and one now does (§3.3b: the Canon 5D III
   float DNG, a float *mosaic*). The change that lifts the refusal carries that test; behaviour that
   no sample has exercised is not accepted.
3. **X-Trans and other patterns** are a `Cfa` with a pattern that is not 2x2; the demosaic stage
   is chosen from the layout (spike: gradient-corrected for Bayer, a generic 6x6 pass for X-Trans,
   `demosaic_cfa.wgsl`). **sRAW and scanner files** are `LinearRgb`: the first stage is skipped
   (architecture §6.4). The pipeline's first stage is chosen by `layout`, and nothing after it
   changes.
4. **Inputs that are not sensor data need a colour space** *(Alice)*. A TIFF scan, a PNG or a JPEG
   enters as `LinearRgb` with an **input profile** (ICC bytes, or a named one: sRGB, Adobe RGB,
   Rec.2020, ProPhoto), since the pipeline must know what colours it is in (spec §5.6, input profiles).
5. **Metadata that is not pixels crosses the sandbox as a language-neutral block, not `serde`**
   *(Alice)*. D-076 makes plugins WebAssembly in any language; a LibRaw plugin is C++ and cannot emit
   a Rust `serde` format (the `NaN` that JSON cannot carry is one symptom). The block is
   **versioned, little-endian, and made of tagged sections (tag, length, payload)**, documented in
   `plugin-api`, and **unknown tags are skipped**. That also answers extensibility: DNG forward
   matrices, baseline exposure, linearisation tables, opcode lists and profiles can be added as tags
   later without breaking the ABI. The 7-word header goes away. The samples keep the existing
   "write into the plugin's memory, read back" path [read: `WasmDecoder::decode`], and the C
   interface stays (architecture §8.2b).
6. **Resident decoded sources** *(Alice)*. About 120 MB per 60 MP photo is acceptable for the photo in
   Develop plus one neighbour. **The engine owns the budget**: a small source cache (least recently
   used, sized from the memory available), and the pipeline may drop its `Arc` under pressure.
   Decoding on demand in strips is **not for M2**. A batch export decodes one photo at a time.
7. `plugin-api` changes are decisions (D-080: experimental until M5, MIT OR Apache-2.0): this is
   **D-141**, reserved by Alice, and no code changes `RawImage` before it is written.

### 3.3 What `rawler` gives on the eight sample files [measured, 2026-09-30]

Run on the CC0 samples of `tools/fetch-samples.sh` (Patrick approved the download; every file
verified by its SHA-256). The eighth file, a Leica M9 DNG, is in the script but not in the spike's
seven.

| File | Layout | Samples | Black (repeat, distinct) | White | Crop origin (x, y) | Matrices | Orientation |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Canon 5D IV (CR2) | **LinearRaw**, 3 components (sRAW) | u16 | 1x1, 1 | 3 x 56032 | (0, 0) | D65, A | Normal |
| Canon R5 II (CR3) | Bayer RGGB | u16 | 2x2, 1 | 14888 | (272, 176) | D65, A | Normal |
| Nikon D850 (NEF) | Bayer RGGB | u16 | 2x2, 1 | 15520 | (16, 8) | D65, A | Normal |
| Sony A7R IV (ARW) | Bayer RGGB | u16 | 2x2, 1 | 15360 | (32, 20) | D65, A | Normal |
| Fujifilm X-T50 (RAF) | **X-Trans 6x6** | u16 | 6x6, 1 | 16383 | (12, **21**) | D65, A | Normal |
| Panasonic S5 (RW2) | Bayer RGGB | u16 | 2x2, 1 | 16380 | (12, 8) | D65, A | Normal |
| Olympus E-M5 III (ORF) | Bayer RGGB | u16 | 2x2, 1 | 4000 | (12, 12) | D65, A | Normal |
| Leica M9 (DNG) | Bayer RGGB | u16 | 1x1, 1 | 16383 | (2, 2) | D65, A | Normal |

What this says, and what it does not:

1. **Averaged black and white levels lose nothing on these eight files** ("distinct" is 1
   everywhere). This corrects a suspicion of §3.1 and demotes §3.2 item 1. The second round below
   finds the one exception (a float DNG), so the point stays low-priority but not closed.
2. **`wb_coeffs` has four values and the fourth is `NaN` on every file** (`[1.99, 1.0, 1.47, NaN]`).
   The spike worked around it by reading three. `RawImage` should carry **three** (R, G, B) plus an
   optional second green, never a `NaN` (which JSON cannot carry, and which no language-neutral block
   should hold either).
3. **The crop origin is not aligned to the pattern**: the Fujifilm's is (12, 21), an odd row, on a
   6x6 pattern. The spike aligned every crop to even coordinates, which cannot work for X-Trans.
   `RawImage` carries the pattern **anchored at the sensor's origin** and the crop separately, and
   the pipeline computes the pattern's phase at the crop (that is what the spike's `cfa_flip`
   approximated for Bayer).
4. **None of these eight has float data, and none is a linear RGB DNG.** The one `LinearRaw` file is a
   Canon sRAW in `u16`. The float gap is closed by the second round (§3.3b); the **16-bit linear
   RGB DNG** is still missing and Patrick has been asked to make one (issue #35).
5. **Orientation is `Normal` on all eight**, and there is **no `blackareas`, no Fuji rotation**. A
   rotated file comes in the second round (§3.3b), with a caveat about what `rawler` reports.
6. Every file gives **both D65 and A matrices**, so the spike's "prefer D65" choice always had a
   D65 matrix to use here; a file without one exists in principle and is not covered.

### 3.3b A second round: nine more files for the gaps [measured, 2026-09-30]

Chosen from raw.pixls.us's repository index and EXIF dumps to fill the gaps of §3.3, downloaded
with Patrick's approval, each verified against the SHA-256 the site publishes. `rawler` 0.8.0 reads
**six of the nine** and refuses three.

| File | What `rawler` reports | Fills which gap |
| --- | --- | --- |
| Canon 5D III "32bit RAW" (DNG) | Bayer RGGB, **`f32` samples**, black `[2047, 2047, 2048, 2047]` (**2 distinct**), one matrix (D65 only), 5920x3950 | **Float samples** exist, so `Samples::F32` has a real file. **Black levels that are not all equal** |
| Fujifilm GFX100S II (RAF) | Bayer RGGB, **11808 x 8754 = 103 MP**, white **65535**, black 256 | A 103 MP stress case beyond the spike's 61 MP; a 16-bit white level |
| Leica M Monochrom (DNG) | **`LinearRaw`, 1 component**, black 220, **no colour matrix, white balance all `NaN`** | A monochrome layout; a file with **no** matrix and **no** white balance |
| Eyedeas E1 (DNG) | Bayer **GRBG**, orientation **`Rotate90`**, no crop, no active area | A non-RGGB phase and a real orientation, in one file |
| Sony DSLR-A450, SLT-A58 (ARW) | Bayer RGGB, orientation reported **`Normal`** | See below |
| Samsung SM-G973U (DNG) | **refused**: "No decoder found, model '', make ''" | A DNG with no `Make`/`Model`: an error path |
| Parrot Bebop (DNG), GoPro HERO6 (GPR) | **refused**: "Unsupported DNG compression" | Files `rawler` cannot decode (GPR is JBIG-compressed) |

What this changes:

1. **`Samples::F32` is now testable** (§3.3, item 4 is closed): the float Canon DNG decodes to `f32`,
   which meets Alice's condition for lifting the plugin's refusal of float data (§3.2, item 2).
2. **Black levels differ within a pattern on one of the fourteen files** that decoded (the float Canon:
   2047, 2047, 2048, 2047). The difference is one count in 2047, which no one will see, so the
   average is still harmless in practice; carrying the pattern remains a cheap precaution, not
   a need.
3. **The orientation `rawler` reports cannot be trusted for every format.** The site's EXIF dump
   says the A450 is `right, top` (EXIF 6) and the A58 `left, bottom` (EXIF 8); `rawler` reports
   `Normal` for both, while it reports `Rotate90` correctly for the Eyedeas DNG. So for ARW the
   orientation is either missing or read from a different IFD. **`imaging` already reads EXIF
   for thumbnails**, so the orientation should come from there, with `RawImage`'s own field kept as
   a fallback, until `rawler`'s behaviour is understood. This is a finding to check with a
   short test on those two files, not a conclusion.
4. **`RawImage` must allow absent data**: no colour matrix and no white balance (Leica M
   Monochrom), a single matrix (the float Canon). `white_balance: Option<..>` and an empty
   `colour` list were already in the proposal; these files show they will occur.
5. **A decoder that refuses a file is a normal outcome** (three of nine), not an exception, so
   the pipeline's caller must handle `DecoderError::Invalid` as a routine case, and the sample
   list should keep at least one such file as a test of that path. LibRaw (as a plugin) might
   read the Samsung DNG that `rawler` refuses; that is a question for the import plugin, not for the
   pipeline.
6. **A monochrome file** is `LinearRaw` with **one** component in `rawler`. The `SensorLayout`
   of §3.2 has a `Mono` variant; the `LinearRgb { components }` variant covers the sRAW file.
   The two Plustek scanner DNGs of raw.pixls.us are likely the same shape, unverified.
7. The GFX at **103 MP** means a 207 MB mosaic; with a full-resolution `Fit` view the pipeline
   already has to band it, and this is the file to check the spike's "61 MP works on a 4 GB card"
   claim against.

Spike 1 measured a decode at 55 to 380 ms, longer than the whole GPU render [measured, report §3b];
what is resident and for how long is §3.2 item 6.

## 4. Where the render service sits (item 3) [agreed]

**A service beside the coordinator**: it writes nothing, and putting it in the coordinator would tie a
50 ms interaction to a thread that sequences every write. Alice owns the engine side. What the GPU
thread needs from the engine:

- **One owner for the device, the queue, the caches and the memory budget** (architecture §4.2),
  on its own thread. The engine holds a `Pipeline` handle and the render requests go through
  channels, as thumbnails do.
- **Priorities that can pre-empt**: an interactive request must not wait behind background work
  that is already on the GPU, so background work is cut into short passes (§6) and the queue is
  re-examined between them.
- **`Invalidate(ImageKey)` on a source change** *(Alice)*: when the original changes (D-019) the engine
  tells the service to drop that image's caches.
- **Decoded source ownership**: the engine decodes and keeps a source cache within a budget it owns
  (§3.2, item 6); the pipeline holds an `Arc<RawImage>` per open photo and may drop it under memory
  pressure.
- **Pixels do not travel through the event bus** *(Alice)*. The engine emits
  `RenderReady { render id, report }` and the interface pulls the bytes by id through an image
  provider, as `thumbs` and `preview` do. Alice settles that last part with Bob when the pipeline API
  exists; it does not change this note. A version to save is `develop`'s business, not the
  pipeline's.

## 5. The operation declaration (item 7)

### 5.1 What exists [read]

`Declaration` (`crates/plugin-api/src/declaration.rs`) has identifier, version, API version,
family, an optional `panel`, an optional `Placement { stage, after, before }` (strings) and
permissions. D-078 also lists parameters with limits and defaults; the crate carries none yet.
`Family` has no `Operation`. There is no place for a shader.

### 5.2 Two layers *(Alice)*

My first proposal put everything an operation needs in one structure. Alice's review splits it in two,
for a reason that holds: the semantics that **sidecars depend on** must not change when an
implementation is optimised, and a pass graph described in data, designed from one spike and one heavy
operation, is guesswork that would risk an ABI break.

**Layer 1: the declaration**, read by `develop`, the version sidecar and the panels. Small and
stable, and it goes into D-078 now (**D-142**):

- identifier, version, API version, **family (`Operation` is missing from `Family` today)**, stage and
  placement (the `Placement` strings become the stage identifiers of the pipeline definition), panel;
- **typed parameters**: type, limits, default and a label key (§2.2 lists the types);
- **input and output data space** (raw, scene-linear, display-referred), which the pipeline can check;
- **cost class** (heavy and rarely changed, or interactive), so `develop` can place an operation, or
  warn, per the order rule the spike proved (white balance after the denoiser: 0.6 ms against 120 ms)
  [open question 17 of the specification];
- permissions.

The sidecar stores `(operation id, operation version, values)` against it, so the parameter types are
needed at M2.

**Layer 2: the implementation descriptor**, read only by the pipeline. **For M2's built-in operations
it is a Rust trait, not data**: `halo(params, scale)`, `passes()`, `draft(params)`. Its **data form is
defined when the first external GPU operation arrives** (M3, with the safety check of D-077), from two
real implementations. What it holds, from what spike 1 measured:

- **Halo**: how many pixels beyond a region the operation reads, **as a function of its parameters
  and of the view scale**. The spike used a fixed 40 px, and its radii scale with the reduced image for
  the fit view [measured: 19 ms against 120 ms]. A fixed number is wrong for both cases.
- **Passes and buffers**: a blur is two passes and intermediate buffers, a denoiser is more.
- **A draft variant**: which parameters change during a drag, and to what (the denoiser's search
  radius of 2 in draft, 5 in final: 32 ms against 120 ms).
- **The shader, the CPU twin and its tolerance**, so a conformance kit can test a plugin as it tests
  the built-ins (testing strategy §4.2).
- **The portable WGSL subset** it stays within, checked before admission (D-077). The DirectX-only
  failure of spike 1 (`sum[c] = ...` through a dynamic index, rejected by FXC) is the reason this is a
  check, not advice.

Mapping of the eight items of my first list: 1, 5 and 6 belong to the declaration; 2, 3, 4, 7 and 8 to
the descriptor (a trait now, data later).

### 5.3 As built (WP13, second pull request)

Layer 1 is in `plugin-api` as D-142 and D-146 decided it. What the decisions left open, and what was chosen:

- **A parameter's spec is its kind plus its limits** (`ParamKind`, one variant per `ParamValue` variant), so a
  limit that makes no sense for a type cannot be written: a bool and a colour have a default only, an int and a
  float have `min`, `max` and a default, an enum has its choices (label keys) and a default index, a point has
  a range per axis and a default, a list has an item kind, a length range and a default, a curve has a
  point-count range and a default. `ParamSpec::default_value()` gives the value an operation starts with, and `ParamSpec::check`
  accepts exactly the values that fit the spec.
- **A declaration is checked in two places.** `Declaration::validate` (this crate) checks what needs only the
  declaration: the fields of an operation are there and consistent, the space names are the API's, parameter
  keys are distinct, defaults satisfy their own limits, and the placement does not name the operation itself.
  What needs the pipeline definition (the stage exists; `after` and `before` name operations **of the same
  stage**, note 006 §3.4) is checked by `develop` at load time, because this crate cannot know the definition
  and must not (it has no dependency on the application).
- **`after` and `before` name operations**, as note 006 §7 asks; the doc comment of `Placement` says so, and
  the validation compares them with the operation's own identifier, where it compared them with the stage
  before.
- **Old declarations still read.** The new fields are optional in the serialised form and absent when empty,
  so a source or an import plugin's declaration is unchanged, and one that gives an operation's field is
  refused (`NotAnOperation`) instead of ignored.
- **Decided in the review of the pull request (Django's questions).** *A parameter's key* starts with a lowercase ASCII
  letter and holds lowercase ASCII letters, digits and `_`, in at most 64 characters: it is the name a value is stored
  under in the sidecar, in an XMP property and in a cache key, so `ev`, `ev ` and `EV` cannot be three parameters, and
  it is the narrowest rule, since loosening it later breaks no plugin and tightening it would (the label key only has to
  say something). *The same operation in `after` and in `before`* is refused by the declaration (`ContradictoryPlacement`):
  it can never be ordered and it needs no definition to see it. *`-0.0`*: `check` allows it wherever zero is, and the
  canonical encoding writes it as `+0.0` (the pipeline's `put_float`, held by a test there), so two equal values make one
  cache key; this crate does not normalise a value. *Unknown fields* in a declaration are ignored, not refused: a newer
  API's fields arrive with its `api_version`, which the host checks, and the required fields fail loudly; refusing the
  rest is for the stable API (M5), and `serde` cannot do it through the flattened parameter specs anyway.
- **The stand-ins in `pipeline`** (`OperationId`, `ParamValue`, the names in `definition::names`) remain until
  Charlie's follow-up (WP14) replaces them with these types. `ParamValue::encode` cannot become a method of
  the `plugin-api` type (the canonical encoding is the pipeline's contract and the type is not in its crate),
  so it becomes a function of `pipeline` that takes the shared type.

## 6. The CPU fallback (item 4) [agreed for M2]

What is known [measured, spike 1]: on Linux, the shared shaders on llvmpipe are about 9 times slower
than the GPU and **faster** than a hand-written Rust path (235 ms against 354 ms on 16 threads). On
Windows, WARP took 3,056 ms for a 24 MP render, **nearly three times slower** than Rust on the same
cores, and 27 times slower than the GPU on the denoiser. macOS always has a GPU.

Position: **decide with a measurement on the machine that matters, not on the runner.** The
runner numbers are noise (testing strategy §7) and the Windows question is about a machine
*without* a usable DirectX 12 adapter, which almost no one has. Agreed for M2:

1. **Shared shaders everywhere**, the pipeline reporting that it runs on a software adapter
   (`RenderReport`), and the interface saying so.
2. **`Pipeline::open` returns a typed error when there is no adapter at all** *(Alice)* (a virtual
   machine, some remote sessions); the report says which **kind** of adapter was used (discrete,
   integrated, software); and the **adapter override** of architecture §6.6 is a setting.
3. The CPU **reference** (which the tests need anyway, testing strategy §4.1) is written for
   clarity. It is a candidate for the fallback **only if** it can be made fast without losing its
   role as reference, which is doubtful: a fast path and a clear one are two things. **No second Rust
   path** unless a real machine needs it: a separate decision, taken if Patrick finds a machine where
   WARP is unusable. Alice writes the decision when the Windows evidence is in.

I cannot measure a Windows machine without a GPU from here.

## 7. Colour (item 5) [nothing to decide yet]

Not measured; listed so that the choices are visible.

- **Working space**: Rec.2020-class, linear, float, fixed by default (architecture §6.5). No
  disagreement.
- **Engine**: lcms2 against moxcms is open. The criterion is stated **before** the comparison
  *(Alice)*, so that the result is a table and not an impression:
  1. **accuracy**: the ΔE (CIEDE2000) of a reference chart's patches through each engine, against the
     reference values, on the same profile pairs (a camera to the working space, the working space to
     sRGB and to a wide-gamut display);
  2. **speed**: the time of a 33³ LUT build and of a 1D-plus-matrix transform on a 24 MP image,
     on one thread and on all;
  3. **`cargo deny`**: the licence and advisory result of each, in this workspace.

  I have not compared them, and I will not recommend one without doing so.
- **Where the display profile comes from**: colord (Linux), the Windows colour system, ColorSync
  (macOS). Per platform, and not judgeable over a remote desktop (D-071).
- **Half-precision intermediates**: halving the memory of every stage buffer [measured: five 60 MB
  buffers for a 2560x1440 chain]. `f16` in shaders is reported as available on this Vulkan adapter
  and not on the GL one [measured, `adapters`]; whether it is available on DirectX 12 through
  FXC and Metal is unknown. A portable alternative is to **store** intermediates as packed f16
  pairs (`pack2x16float`, core WGSL) and compute in f32: no extension needed. Its effect on quality
  is the open part; I would measure banding on a ramp and the error accumulated through the chain.

## 8. The denoiser (item 6) [nothing to decide yet]

Non-local means was a yardstick [read: architecture §14, risk 10]: 108 ms of the 121 ms chain, and
its cost grows with the square of the search radius (32 ms at radius 2, 420 ms at radius 9)
[measured]. It is one operation behind a declaration (§5), so **choosing it does not change the
pipeline**. What I need before proposing one: a quality criterion (noise removed against detail
kept, on the real samples) and the AI denoiser's expected cost (M5). Until then the pipeline
should not depend on non-local means, and the draft variant (§5.2, layer 2) is what keeps it
inside the budget.

## 9. Order of work

Each step its own pull request. Alice's order: **step 2 starts now**, in parallel with this note,
because it does not depend on the `RawImage` decision and it is the riskiest part of continuous
integration (three platforms, software adapters).

1. **`RawImage`** (D-141, then the `plugin-api` change with the tagged-section block, the decoder
   plugin and the host, with a fixture per sample file, and the test that lifts the float refusal).
2. **`crates/pipeline`**: the crate, its entry in the `ALLOWED` table of `xtask`, the adapter choice
   with its typed error, the device on its own thread with error scopes and device-loss handling, and
   a first smoke test on lavapipe, WARP and the runner's Metal adapter.
3. **The first stages** as a recipe: decode input, demosaic, exposure and white balance, tone and
   display, each with its CPU reference and its golden render.
4. **`xtask gpu-check`**: adapters, reference and smoke tests, the harness, one JSON file (CI doc
   §4).

## 10. What is decided, and what is not

- **Accepted** (issue #34): the shape of §2 and §3 with the amendments marked above, the two layers of
  §5, the service beside the coordinator (§4), and the fallback for M2 (§6).
- **Written** (2026-09-30): D-140 (recipe, render API, dependency edges), D-141 (`RawImage` ABI) and D-142
  (declaration and parameter types), with architecture §3.1 (the edge), §4.3 (renders are not writes), §6.1
  (the recipe), §8.3 (the two layers) and risk 5 changed to match. This note is the record of the
  reasoning, not the decision.
- **Not decided**: the colour engine and the denoiser (§7, §8), the descriptor's data form (M3), and
  what to do about a machine without a usable graphics adapter beyond the typed error (§6).
- **Patrick**: a CC0 **16-bit linear RGB DNG** is still missing (issue #35). Nothing else pending.
