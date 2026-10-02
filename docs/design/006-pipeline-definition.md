# Design note 006: the pipeline definition v1 and the base look

> **Status: accepted, with amendments, and decided in D-146.** **D-146**, which Alice enters in the
> [decision log](../decisions.md) when this note is merged (it adopts the definition v1, the five data
> spaces, the placement language, the fingerprint rule and the Rust data form, and amends D-142 and D-141),
> is the authority; this note is the record of the reasoning. It was a proposal, accepted with amendments in
> [#45](https://github.com/Straycat-Software/auroraw/issues/45) and in the review of the pull request that
> merged it. Written by Charlie (an AI assistant, Claude Code; image processing and the GPU) with Alice, as
> [milestone M2's plan](../m2-plan.md) (§6, items 1 to 3; §11, item 3) asks. It answers specification
> question 17 (what a pipeline definition is, which stages, which data spaces, which ordering constraints)
> and gives the structure of the base look (question 23). It builds on
> [note 005](005-image-engine-interfaces.md) (the recipe and the declaration, D-140 and D-142). The
> amendments of Alice's reviews are folded in, each marked *(review of this note)*.
>
> Everything is tagged **[measured]** (a number, from spike 1 or from a run on the reference machine, said
> which), **[read]** (a fact in the code or a document) or **[proposed]**. Nothing here is built.
>
> **Three results** (§2.2, §4). The spike said that white balance applied before or after the denoiser gives
> the same image; it does not, as soon as the denoiser is on, so the order is part of what a definition
> promises. The quality experiment that note asked for shows the two orders **within a few tenths of a
> decibel of each other** with the spike's denoiser (on average; single cases between -0.44 and +0.35 dB) at
> the best strength of each, while the cost of a slider differs by a factor of fifty. And a third arm, asked
> for in review, gives the denoiser a **noise model per channel**: then the two orders give **the same
> image** (0.001 dB apart, which algebra predicts), the best strength is **stable in noise units** (1.25 to
> 1.5 across noise levels) and the quality is a little better than with a raw strength. **v1 therefore
> places white balance after the denoiser**, which adds a stage (§3.3), and the note proposes that the
> denoiser's strength be in noise units (§4.5).

## 1. The question

A **pipeline definition** (specification §5.6) is "a documented, versioned list of stages, each working
in a defined data space". The recipe (note 005 §2.2) carries its version and the operations in order; the
version sidecar records it (architecture §7.2) so that an old edit renders the same after an update.
Four things have to be fixed before `develop` can place an operation and the pipeline can validate one:

1. **Which stages**, in which order, and what data each one receives and returns.
2. **What the definition owns** and what the recipe owns: the parts of the chain that are always there
   (applying the black level, demosaicing, the camera-to-working step, the display transform) against the
   operations a person can change.
3. **How an operation is placed** inside its stage: the ordering-constraint language, and what happens when
   it cannot be satisfied.
4. **What makes a new version**, and what a released version promises.

## 2. What is known

### 2.1 The order rule [measured, spike 1, GTX 1650 SUPER]

Heavy, rarely changed operations go early; controls dragged often go late, because a change reruns its
stage and every later one from the cache of the one before.

| A change of... | Time at a 2560x1440 view | Stages rerun |
| --- | --- | --- |
| exposure, tone curve | 0.7 ms | tone |
| sharpening or local contrast amount | 2.6 ms | combine, tone |
| denoise strength | 118 ms | denoise and later |
| white balance, **before** the denoiser (in the demosaic) | 120 ms | everything |
| white balance, **after** the denoiser (folded into the camera matrix) | 0.6 ms | tone |

A **draft quality** (a denoiser search radius of 2 instead of 5) costs 32 ms instead of 120 ms and keeps a
drag under the 50 ms budget.

### 2.2 Are white balance before and after the denoiser the same image? [measured, 2026-09-30]

Spike 1's report says of the cheap variant: "the result is the same, since the multipliers fold into the
camera matrix". That is true of linear operations and was never checked through the denoiser, which is
not linear: it compares patches, and a patch does not look the same before and after a per-channel
multiplication. I measured it, with spike 1's own chain (demosaic, non-local means with a search radius
of 5 and patches of 3x3, blurs, combine, tone) on the GTX 1650 SUPER: for each file, the centre 512x512
region rendered twice, once with the white balance applied in the demosaic (the denoiser sees balanced
data) and once with the demosaic left in camera values and the multipliers folded into the matrix of the
tone pass, then compared in 8 bits (the smoke test's own comparison).

| File | As-shot multipliers (R, B) | Denoiser off (control): max level / channels over one level | Denoiser on, `h` = 0.03 (spike 1's value): mean level / channels over one level / max level |
| --- | --- | --- | --- |
| Nikon D850 | 1.90, 1.38 | 3 / 0.002 % | 0.59 / 10.4 % / 54 |
| Olympus E-M5 III | 2.19, 1.64 | 1 / 0.000 % | 0.52 / 9.2 % / 41 |
| Panasonic S5 | 2.00, 1.92 | 1 / 0.000 % | 0.30 / 3.6 % / 76 |
| Leica M9 | 1.38, 2.00 | 1 / 0.000 % | 0.08 / 1.1 % / 32 |
| Sony A7R IV | 3.07, 1.50 | 1 / 0.000 % | 0.09 / 0.8 % / 12 |
| Canon R5 II | 1.83, 1.76 | 1 / 0.000 % | 0.01 / 0.006 % / 7 |

What it says, and what it does not:

1. **The fold is exact without the denoiser** (the control column: at most three levels, on 0.002 % of the
   channels): the matrix arithmetic is right, and the difference below is the denoiser's.
2. **With the denoiser on, the two orders are two different images**: from almost identical (Canon R5 II)
   to 10 % of the channels more than one level apart (Nikon D850), with single pixels up to 76 levels
   apart. With `h` = 0.01 the Nikon goes to 42 % of its channels (117 levels at most); with 0.06 it falls to
   1.2 %. The same strength does not remove the same amount of noise in both orders.
3. **It does not say which is better.** One region per file, one strength per row, the same `h` in both
   orders (and `h` means something different per channel once the channels are scaled): this is a measure
   of *difference*, not of quality. The quality comparison is the experiment of §4, which also shows that
   the best strength in the "after" order is about 0.6 times the best strength in the "before" order.
4. **It changes what the order means for a saved edit.** A recipe stores a denoise strength; the image it
   gives depends on where the white balance sits. So **the position of white balance in a definition is part
   of what a version promises**, exactly as the plan's determinism criterion requires, and cannot be changed
   inside version 1 after edits exist.

### 2.3 What the stage boundaries are for [read, architecture §6.2]

The output of each stage is cached for the region and quality on screen; a change reruns its stage and
every later one. **A stage boundary is therefore a cache boundary**: where the definition puts a
boundary decides what a slider costs, and an operation inside a stage has no cache of its own.

## 3. The definition v1 [proposed]

### 3.1 What it is and what it holds

A definition is an **immutable, versioned value**: a number, the ordered list of stages, the data space
at each boundary, the working space, the **fixed spine** (the steps that are always there, below) and
the **canonical order of the built-in operations** in each stage. It holds no parameters, no plugin and no
mask (note 005 §2.2: the definition version is the extension point).

- **The definition owns the spine; the recipe owns the operations.** The spine is what every render
  does and a person cannot remove or reorder: applying the levels, demosaicing (chosen by the input's
  layout), the camera-to-working step, the orientation, the display transform. Its inputs come from the
  image (`RawImage`) and the output (`OutputTransform`), not from the recipe. The recipe lists only what a
  person can change, which is what `develop` stores and the sidecar keeps. A second reason: a spine step
  written into every recipe would be duplicated data to keep equal in every sidecar.
- **Both ends of the chain are outside it.** Decoding is the decoder plugin's (D-141): the first stage
  receives a `RawImage`. Encoding is `export`'s: the definition ends at display-referred pixels, which
  note 005 §2.3 hands to a sink. M2's plan lists `decode` and `encode` among the stages; I treat them as the
  boundaries of the definition, not stages in it, because neither runs on the pipeline's GPU thread.

### 3.2 Data spaces

**The names of the five spaces and of the stages are constants in `plugin-api`** (review of this note): a
declaration names a stage and a space, `plugin-api` depends on nothing, and `develop` must refuse an unknown
one, so they are part of the public surface, listed there with their meaning; `pipeline`'s definition
refers to them, and a test checks that its table equals the constants, so that an external author and the
definition cannot drift apart.

D-142 names three spaces (raw, scene-linear, display-referred) for an operation's input and output. The
chain passes through five states that matter for whether an operation is correct, so I propose five, each
one a refinement of D-142's three:

| Data space | What it is | D-142's space |
| --- | --- | --- |
| `sensor-raw` | The decoder's samples, as counts, one per photosite (or per component for a linear input), with the black and white levels not yet applied | raw |
| `mosaic-linear` | One linear value per photosite, levels applied, normalised so that the sensor's white is 1.0, the colour filter pattern still present; **a sample below its black stays negative** | raw |
| `camera-linear` | Linear RGB per pixel, **in the camera's primaries**, **no upper bound and no lower bound**: a noise-model denoiser needs the samples that noise took below the black level, and the demosaic keeps them. **The white balance is applied inside `input-colour`, by its first operation** (§4): the stages before it read unbalanced data, and what follows it in that stage (the highlight reconstruction) reads balanced data, so a declaration that reads `camera-linear` says only the primaries and that the data is linear | scene-linear |
| `working-linear` | Linear RGB per pixel in the **working space's primaries** (§5), scene-referred, no upper bound | scene-linear |
| `display-referred` | Values in the output space's encoding, bounded to 0..1 | display-referred |

The split of scene-linear into two matters because an operation written for the working primaries
(saturation, hue-saturation-luminance, colour grading) is wrong on camera primaries, and the declaration's
input space is what lets the pipeline **refuse to place it there** instead of rendering it wrongly.

### 3.3 The stages of v1

Eight stages, in this order. "Spine" is what the definition owns; "operations" are what a recipe may
list, with the built-in ones in their **canonical order** (§3.4). The operation names are the plan's slices
(WP15) and are provisional until that package fixes the identifiers.

| # | Stage | In → out | Spine | Operations (canonical order) |
| --- | --- | --- | --- | --- |
| 1 | `raw-linear` | `sensor-raw` → `mosaic-linear` | black and white levels (a repeating pattern, D-141) | hot pixels |
| 2 | `demosaic` | `mosaic-linear` → `camera-linear` | demosaic by layout (Bayer, X-Trans, a linear input passes through) | none in M2 |
| 3 | `camera-rgb` | `camera-linear` → `camera-linear` | none | noise reduction |
| 4 | `input-colour` | `camera-linear` → `working-linear` | **camera to working space** (after the operations) | **white balance**, highlight reconstruction |
| 5 | `scene-linear` | `working-linear` → `working-linear` | none | exposure and black point, tone (contrast, highlights, shadows, whites, blacks), curve, saturation and vibrance, hue-saturation-luminance, colour grading |
| 6 | `geometry` | `working-linear` → `working-linear` | orientation and the recommended crop (from `RawImage`) | crop, straighten |
| 7 | `detail` | `working-linear` → `working-linear` | none | sharpening |
| 8 | `display` | `working-linear` → `display-referred` | the output transform (display or export profile, note 005 §2.4) | **tone map** (the base look's curve, §6) |

**Why `input-colour` is a stage of its own.** A stage boundary is a cache boundary (§2.3) and an
operation inside a stage has no cache of its own. For white balance to cost 0.6 ms instead of the
denoiser's 120 ms (§2.1) it must sit **after** the boundary that follows the denoiser, **and nothing
before that boundary may read it** (the rule at the end of §3.4). That is also why **highlight
reconstruction** is in this stage, after white balance, and not in `camera-rgb` as the first version of this
note had it. White balance is a
per-channel multiplication that folds into the camera-to-working matrix (§2.2, the control column), so the
stage does one 3x3 multiplication per pixel whatever the recipe says. The operation keeps its own entry
in the recipe (a person moves its sliders; the sidecar stores its values) and the implementation folds
it into the spine's matrix.

Differences from the plan's proposed list, each with its reason:

- **`denoise` becomes `camera-rgb`.** It is the home of the operations on camera RGB that **do not need the
  colour interpretation** (today noise reduction alone; the camera-space corrections of M3 would join it), so
  it is not named for one operation. It is not called `camera-linear`, which is the name of the **data space** it reads and returns (review of this
  note): "an operation in `camera-linear`" must say one thing. The same reason names stage 4
  `input-colour` (the colour interpretation of the camera data: the balance and the matrix) and not
  `camera-colour`, which would sit too close to `camera-rgb`.
- **`geometry` comes before `detail`** (review of this note), at `working-linear`, where resampling in
  linear light is the correct one: sharpening then acts on the final pixel grid instead of sharpening a
  grid that the straighten interpolates and softens again, and a crop restricts the work of everything
  after it. The cost is that dragging a crop reruns `detail` and `display`; spike 1 measured sharpening
  at 2.6 ms.
- **`decode` and `encode` are the boundaries**, as above.
- **`display` holds the tone map**, so that the flat linear look of D-042 is the same definition with
  that operation absent: raw linear data under the output transform, and nothing else.
- **White balance is not in `raw-linear`** (the plan's proposal) **but in `input-colour`, after the
  denoiser**: the quality experiment of §4 found no quality reason to keep it before.

### 3.4 How an operation is placed: the ordering constraints

- **An operation declares its stage** (`Placement.stage`, which D-142 turns into a stage identifier) and
  may declare `after` and `before`. **They name operations (by identifier), not stages**, and **apply
  within the operation's stage only**: between stages the order is the definition's. Today
  `Placement::after` and `before` are documented as naming stages; D-142 is the place to say they name
  operations (§7).
- **They are optional and relative**: "after `auroraw.exposure`" is satisfied whether or not that operation
  is in the recipe, which is why the constraint is written against an identifier and not a position.
- **The built-in operations need no constraint**: the definition lists them in canonical order, per stage.
  An operation that declares nothing goes **at the end of its stage**, among several of them in identifier
  order, so that the placement is the same on every machine and in every version of Auroraw (specification
  §5.6: "a plugin with no constraint goes at the end of its stage").
- **Conflicts are errors at load time, never silently resolved**: a cycle among the constraints, two
  constraints that cannot both hold with the canonical order, or an `after` and a `before` that exclude
  each other. The error names the operations and the stage; the operation is refused and the rest of the
  recipe stays valid.
- **A constraint that names an operation of another stage is an error at load time**, not silently ignored
  (review of this note); there is a test for it. Nothing relates operations of different stages, so no
  constraint can move an operation out of its stage (the spec's "an operation cannot leave its stage").

**What an operation may depend on** *(review of this note)*. The cache key of stage *n* is the hash of the
recipe up to it (note 005 §2.2), and a change reruns its stage and every later one. What a slider costs is
therefore decided by **what each operation reads**, and the rule is:

> **An operation depends only on its own parameters, on the operations before it in the definition's
> order, and on the image.**

"The image" is `RawImage` and what it carries, which no slider changes (the levels, the **as-shot**
multipliers, the matrices, the ISO). The consequence that matters here: **nothing in stages 1 to 3 may read
the white balance**, because if one did, a balance drag would rerun stage 3, the denoiser included, and the
0.6 ms of §2.1 would be 120 ms again. An operation that needs the balance goes **after** it.

- **Highlight reconstruction for M2** [proposed]: it needs to know what "neutral" is, since the clip point of
  each channel in balanced space is its multiplier, so it sits **in `input-colour`, after white balance**,
  and reads the recipe's balance, which is an operation before it in the definition's order. The simple
  clip-aware form is **per pixel**, which I expect to cost about what the balance itself costs, so a drag
  reruns it with no visible price (not measured: the algorithm is WP15's). The two alternatives, and why
  not: staying in `camera-rgb` with the **as-shot** multipliers taken from `RawImage`, which no slider
  changes, avoids any rerun but gives a reconstruction that **ignores a corrected balance**; and a
  reconstruction that needs no balance (each channel limited at its own saturation) **reconstructs no
  colour**.
- **A heavier algorithm would bring the price back.** A reconstruction that works over a neighbourhood
  (inpainting from the unclipped surroundings) rerun on every balance change costs what its neighbourhood
  costs. The rule does not forbid it; it makes the price visible and places it: the operation declares the
  dependency by sitting after the balance, and its **cost class** (D-142, layer 1) lets `develop` warn when a
  heavy operation is placed after one that is dragged.
- **A test holds it**, in `pipeline`, with a counting cache (testing strategy §4.6, counts and not times): **a
  change of the white balance reruns `input-colour` and the stages after it and nothing earlier**, and
  more generally a change in an operation of stage *k* reruns the stages from *k* on and none before.

### 3.5 What is checked, and by whom

`develop` **places** (D-140); the pipeline **validates** what it is given and refuses, with a typed error,
a recipe in which:

1. an operation's stage is not in the definition the recipe names;
2. the stages are not in the definition's order, or an operation is inside a stage other than its own;
3. within a stage, the order breaks the canonical order or a constraint;
4. an operation's declared input space is not what the stage delivers at that point (a working-space
   operation in `camera-rgb`);
5. the same operation appears twice where its declaration allows one;
6. an `op_version` names no implementation the registry holds (the operation is skipped as
   *disabled and marked*, architecture §7.2, not an error).

The first five make a recipe invalid; the sixth makes one operation inert. Both are tested with
hand-written recipes, which is the reason the pipeline does not place.

### 3.6 Versions: what makes a new one, and what a released one promises

- **A new definition version** is needed for any change that can alter the image a saved recipe gives:
  a stage added, removed or reordered; a stage's data space; the working space; the **position of a
  built-in operation** (§2.2); the spine's content or its maths; the canonical order. A change to an
  operation's own behaviour is **not** a definition change: it is the operation's `op_version`
  (architecture §7.2), and both are recorded in the sidecar.
- **A released version is never edited.** The engine keeps every released definition, which are a few
  dozen lines each, and renders a recipe with the definition it names. A test holds a **fingerprint** (a
  `blake3` of the definition's canonical encoding, the same encoding rule as the recipe's, note 005 §2.2)
  for each released version: any change to a released definition fails it, which is the mechanism that
  keeps "no edit of version 1 after release" from depending on a person's memory. The fingerprint covers the
  stage list and order, the data spaces, the working space, the canonical order **and the identity of the
  spine** (its steps and their order), since a change of "the spine's content or its maths" is a reason for a
  new version and a test can only see what the fingerprint covers (review of this note).
- **It enters the recipe's hash** through the version number, so a cache entry for one definition is never
  served for another.
- **The promise is the bound of testing strategy §4.2**, not identical pixels (M2 plan §9, risk 3): the
  same recipe on the same definition gives the same image within one 8-bit level on 99.9 % of the pixels,
  on every adapter.
- **When v1 freezes.** The plan's increment A does not save edits, so nothing written with v1 outlives it;
  **v1 freezes at the start of increment B** ("I edit and keep"), once the working space (§5) is settled and the
  denoiser's choice (plan §6, item 8) has confirmed the placement of white balance (§4.6). Until then it may change, and each
  change is a commit to a file, not a version.

### 3.7 Where it lives

In `crates/pipeline`, as Rust data (a `const` table) with a documented text rendering in `docs/`, **not** as
a file read at start-up: v1 does not need a parser, and a parser of a configurable file brings a format
to keep, a fixture and a fuzz target (M2 plan §4). The spec's "advanced users configure the pipeline
definition, a documented, shareable file" (§5.6) is for later and uses this structure as its schema; that
is the moment to choose its syntax. I note it so that it is a choice made on purpose.

## 4. White balance and the denoiser: the quality experiment [measured, 2026-09-30]

The plan placed white balance in `raw-linear`, before demosaicing and denoising; spike 1's rule places it
after the denoiser, where a slider costs 0.6 ms instead of 120. §2.2 shows that with the spike's denoiser the
two are **different images**, so the choice had to be made on quality first and cost second. The experiment
described here has been run, with three arms: the spike's denoiser in both orders, and (review of this
note) a denoiser with a noise model in both orders.

### 4.1 Method

- **A clean reference from real data.** For each real file and region, a **2x2 binning of the mosaic**:
  each output photosite is the mean of the four same-colour photosites of the matching 4x4 block, so the
  result is again a mosaic of the same pattern, at half the resolution and with the file's own noise
  roughly halved. **Primary set**: the low-ISO files, where the reference is cleanest: Sony A7R IV (ISO 50),
  Nikon D850 (ISO 64), Olympus E-M5 III (ISO 200). **Sensitivity set**: Canon R5 II, Panasonic S5 and Leica
  M9, all at ISO 640, whose reference is noisier. Three regions per file (a 640x640 binned mosaic each, the
  central 512x512 compared).
- **Synthetic noise, added in the raw domain** at three levels, Gaussian with the variance of a
  Poisson-Gauss sensor, `var = A·v + B` on the normalised signal `v`: as counts of a 14-bit sensor, `A`
  of 0.5, 4 and 16 counts per count and a read noise of 3, 8 and 24 counts (**L1, L2, L3**: stand-ins for
  roughly ISO 400, 1600 and 6400, an order of magnitude and not a measured camera model). The same noisy
  mosaic goes through every arm.
- **The chain** is spike 1's: demosaic (gradient-corrected), non-local means (search radius 5, patches
  of 3x3), tone; sharpening, local contrast and the mask are off. **Both orders**: the multipliers applied
  right after the interpolation, so that the denoiser sees balanced data (before), and the demosaic left in
  camera values with the multipliers folded into the matrix (after).
- **The four arms.** *Early* and *late*: the spike's denoiser, **one raw strength `h` for the three
  channels**. *Early-VST* and *late-VST*: the same search **in a variance-stabilised space**: a
  generalised Anscombe transform per channel, `f = 2·sqrt(y/a + 3/8 + b/a²)`, then the average inverted
  with the closed-form asymptotically unbiased inverse (Mäkitalo and Foi, 2011). The channel's model is
  `(a, b)`: in the late order `(A, B)` for all three; in the early order the channels have been scaled by
  the multipliers `m`, so `(A·m, B·m²)`. The transform gives every channel, at every level, a noise of
  standard deviation 1, so **`h` is in noise units**. (The transform and its inverse were checked on
  Monte-Carlo draws first: variance 1.0 from three photo-electrons up, the closed-form inverse within 0.05
  of the truth, the naive algebraic inverse low by about 0.25.)
- **A sweep of the strength** (18 values for the raw arms, from 0.0001, the denoiser off, to 0.45; 13 for
  the VST arms, from 0.05 to 6), **each arm at its own best**. Comparing at the same strength would be unfair
  (§2.2, point 3).
- **The measure** is in the 8-bit sRGB output, against the clean mosaic rendered with the denoiser off:
  **PSNR** on RGB, on luma (Y) and on chroma (Cb and Cr together), and **SSIM** on luma (8x8 windows).
  The scratch tool and its CSV are not committed; the method above is what reproduces it. A re-run of the
  two raw arms with the extra arms present gave the same 1,944 values as before.

### 4.2 Result 1: with the raw strength, the orders are within a few tenths of a decibel

Primary set, nine cases per level (three files, three regions). "Late" is white balance after the
denoiser; a positive difference favours it. "Better / worse / tie" counts cases at ±0.05 dB of RGB PSNR.

| Noise | Late minus early, RGB PSNR | Better / worse / tie | Luma | Chroma | SSIM (luma) | Best `h`, late ÷ early (median) | What the denoiser gains |
| --- | --- | --- | --- | --- | --- | --- | --- |
| L1 (light) | **-0.08 dB** | 1 / 5 / 3 | +0.01 | -0.12 | +0.0001 | 0.50 | +0.95 dB |
| L2 | **+0.14 dB** | 6 / 1 / 2 | +0.48 | -0.06 | +0.0105 | 0.60 | +3.85 dB |
| L3 (heavy) | **+0.23 dB** | 9 / 0 / 0 | +0.70 | -0.16 | +0.0310 | 0.62 | +6.82 dB |

Per file at L3 (mean of three regions, range): Sony +0.23 dB [+0.18, +0.26], Nikon +0.22 [+0.18, +0.25],
Olympus +0.24 [+0.18, +0.28]. At L2: Sony +0.28, Nikon +0.01, Olympus +0.12. Sensitivity set (ISO 640
references): RGB -0.03, +0.03, +0.05 dB at L1, L2, L3; chroma -0.04, +0.01, +0.02. Looked at by eye on the
Sony, Nikon and Olympus crops, the denoised pictures of the two orders are very close, the "after" one
keeping slightly more texture. In single cases the difference runs from -0.44 to +0.35 dB (25 of 27 within
±0.3 dB).

### 4.3 Result 2: with a noise model, the orders are the same image

Early-VST and late-VST differ by **at most 0.001 dB** of RGB PSNR over all 54 cases and all 13 strengths.
That is what the algebra says and not a coincidence of the data: the transform's inputs, `y/a` and `b/a²`,
do not change when `y` is multiplied by `m` while `a` becomes `a·m` and `b` becomes `b·m²`, so the denoiser
sees **exactly the same numbers** whichever order the multipliers were applied in, and the inverse scales
the result back. **For a denoiser with a per-channel noise model the position of white balance is
irrelevant to quality**, and the choice is the cost alone.

### 4.4 Result 3: the noise-model denoiser is a little better, and its strength is stable

The VST arm (one column, since the two orders are equal) against the two raw-strength arms, each at its own
best, primary set:

| Noise | RGB PSNR: early / late / **VST** | VST minus late (better / worse of 9) | Luma | Chroma | SSIM (luma) | Best `h` in noise units |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | 36.98 / 36.90 / **37.18** | **+0.28 dB** (9 / 0) | +0.24 | +0.29 | +0.0010 | 1.25 (8 of 9), 1.5 |
| L2 | 30.82 / 30.96 / **31.31** | **+0.35 dB** (8 / 0) | +0.28 | +0.37 | +0.0036 | 1.25 (8 of 9), 1.5 |
| L3 | 27.63 / 27.86 / **28.02** | **+0.16 dB** (6 / 2) | +0.19 | +0.14 | +0.0066 | 1.25 (6 of 9), 1.5 |

- **The chroma weakness of result 1 is gone**: the VST arm is better on chroma at every level (+0.14 to
  +0.37 dB), where the late raw-strength arm was worse than the early one.
- **The strength is stable in noise units**: 1.25 or 1.5 in all 27 primary cases, where the raw
  strength of the spike's denoiser varied about eightfold between the light and the heavy level (the mean
  best `h` of the early arm was 0.0015, 0.0050, 0.0127). A slider in noise units means the same thing at
  every ISO, in every channel and in both orders.
- **The sensitivity set agrees in direction**: VST minus late +0.52, +0.55, +0.23 dB (9 / 0, 8 / 0, 6 / 1
  better / worse). Its best strength is larger and more spread (median 1.0, 1.5, 2.0; range 1.0 to 4.0), as
  expected when the reference's own noise, at ISO 640, is not in the model.
- **By region the gain is uneven**: at L3 on the Sony it is +0.71, -0.23 and +0.04 dB; on the Nikon +0.08,
  +0.06 and -0.11; on the Olympus +0.19, +0.32 and +0.39.

### 4.5 What it supports, and what it does not

1. **Quality does not separate the orders** with the raw strength (0.3 dB on average), and **does not
   separate them at all** with a noise model (0.001 dB). **There is no evidence that balancing before the
   denoiser is better.** The cost is not close: a white balance slider costs 0.6 ms after the denoiser and
   about 33 ms before it even with the draft variant (§2.1), 120 ms at final quality. **The cost decides.**
2. **The strength of a raw-`h` denoiser means something different in each order** (the best `h` after is
   about 0.6 times the best `h` before), so a stored strength, a style or a preset written for one order
   cannot be applied in the other. With a noise model it means the same in both, which is one more reason to
   prefer one.
3. **It supports a denoiser with a noise model, without choosing one.** It is a little better here (+0.16 to
   +0.35 dB, better on chroma, better in 23 of 27 primary cases) and its strength is in units a person and a
   sidecar can rely on. It is input to M2 plan §6, item 8 (the denoiser's choice), not its answer.
4. **What that means for the declaration (layer 1, D-142).** The denoiser's strength parameter would be **in
   noise units**, and the operation needs a **noise model as an input** that is not a slider: `(a, b)` per
   channel, from somewhere. Three sources exist: the **`NoiseProfile` tag of DNG** where a file carries one;
   a **profile table per camera and ISO**, which is how darktable's profiled denoising works (per the
   review); or a **blind estimate** from the image. **Nothing in `RawImage` (D-141) says the ISO today**
   (the catalogue knows it, but the decoder's output is not the catalogue's), and D-141's tagged sections
   carry an `iso` tag and the DNG noise profile **without breaking the ABI**: D-146 amends D-141 so that they
   do, **when the file has them**, in WP13 with the rest (review of the pull request). The denoiser's
   declaration itself (the strength in noise units, the model an input that is not a slider, a blind estimate
   when the file has no profile) is settled **with the denoiser's choice** (plan §6, item 8), not here.
5. **The limits**:
   - the noise is **synthetic, independent and Gaussian**, and **the noise model is handed to the
     denoiser exactly as it was generated**: this is the best case of the third arm. A profile that is wrong
     or estimated would cost some of the gain, and how much is **not measured**;
   - the VST applies to the **demosaiced** data, whose noise is correlated and of smaller variance at the
     interpolated sites than the model says; the optimum strength absorbs that;
   - **the "before" arm applies the multipliers after the interpolation**, so that the denoiser sees balanced
     data; applying them **on the mosaic, before the demosaic** (the plan's original placement) was not
     tested, and the gradient-corrected demosaic mixes channels, so it would not commute exactly;
   - it is **one denoiser family**, non-local means; another (wavelets, a learned one) could behave
     differently, and the denoiser is not yet chosen;
   - **PSNR and SSIM are not perception**, and the best PSNR over-smooths: the pictures at those strengths
     are softer than anyone would choose;
   - **three files** in the primary set, three regions each, which are not independent: the evidence is
     the direction and its consistency, not a confidence interval; the references are not perfectly clean
     (binning halves the file's own noise; the Leica M9 at L1 gets almost nothing from the denoiser in either
     order).

### 4.6 Decision proposed

**White balance goes after the denoiser in v1**, in the `input-colour` stage of §3.3, folded into the
camera-to-working matrix: the quality evidence is neutral to slightly favourable, the cost evidence is
decisive, and with a noise model the question disappears. It stays **reversible until v1 freezes** (§3.6).
The experiment is worth keeping as a script of the verification package (WP24), with its scene generator and
its noise model, so that a denoiser candidate is compared by the same table; **it only needs re-running if
the chosen denoiser has no per-channel noise model**, since for one that has, the order is irrelevant by
construction.

## 5. The working space [measurement planned, M2 plan §6 item 2]

The working space is **part of the definition**: changing it is a new version. v1 proposes **linear
Rec.2020 primaries with the D65 white**, as architecture §6.5 and the plan do, **provisionally**: the plan
asks that it be confirmed against linear ProPhoto RGB by measurement in increment A, **before the
scene-linear operations (15b, 15c) are written against it**. The measurement is the plan's (a) to (d):
the share of values negative or clipped after the camera-to-working step, per file (the plan says seventeen files; fourteen decode), and on saturated blues
and greens; the hue shifts of saturation, vibrance, hue-saturation-luminance and colour grading on those
colours; the cost and the error of the D50-to-D65 adaptation on the way to sRGB and Display P3; and
Patrick's judgement of the borderline photos on his screens. Charlie measures, Patrick judges. If
ProPhoto wins, only this table and the spine's matrix change, because nothing is written against the
space until then. **The measurement needs the colour engine's matrices and so waits for the first stages**;
it is not done here.

## 6. The base look [structure proposed; constants not chosen]

D-042: an unedited photo is shown with a **base look**, a style (D-040) applied to the default version,
neutral by default, with a **flat linear** one among the choices; no version is written until the first
edit. Specification question 23: the tone-mapping method, and how the flat linear look is presented.

**Structure** [proposed]:

- The base look's tone curve is **one operation, `tone map`, in the `display` stage**, between the
  scene-linear image and the output transform. It maps the scene-linear, unbounded values of
  `working-linear` to `display-referred`, and is the only place where the scene's dynamic range is
  compressed (D-041: a single display transform). A style sets its parameters; a version's recipe carries
  them like any other operation's.
- **The curve is a parametric toe and shoulder** with a pivot at middle grey (0.18 in scene-linear maps to a
  chosen display value), documented by its formula and its parameters so that it is a *documented method*
  (the plan's wording), with its stated limits: a toe that keeps the blacks from clipping, a shoulder
  that rolls off the highlights, **monotonic**, with a slope at the pivot that the contrast parameter sets.
- **Neutral** is one set of parameters: the faithful, unsurprising rendering. **Flat linear** is **the
  `tone map` operation absent**: the working-space data under the output transform, which clips
  above 1.0 and shows the raw linear look. Because it is the same definition with one operation missing,
  it needs no special case in the pipeline or the sidecar. How it is presented to the photographer, and
  the wording "deliberately flat", are the interface's to settle.
- **The default recipe contains the neutral `tone map`** (review of this note). D-042's "no version is
  written until the first edit" means that an unedited photo is rendered from the default recipe, and that
  recipe **holds** the neutral `tone map`; an absent `tone map` and the neutral one are then the two base
  looks. **`develop` must not treat a missing `tone map` as an error**: it is the flat linear look.
- **The photographer's tone curve stays a scene-linear operation** (§3.3, stage `scene-linear`; review of
  this note asked which I intend). Its editor cannot be a curve on a 0..1 axis, since the scene-linear
  values are not bounded: I intend **a curve on a logarithmic axis with a pivot at middle grey**, which
  is how a scene-referred curve is usually drawn. The alternative, a curve on display-referred values,
  would have to act in an **encoded** space after the output transform, which splits the `display` stage's
  data spaces in two and makes the curve depend on the output profile. The editor itself is a question for
  the Develop view (Bob); this note only fixes where the operation sits and what its axis is.
- The look a camera's embedded JPEG gives is out of scope (D-042: planned, more likely a plugin).

**Not chosen here**: the curve's formula and its constants. They cannot be picked honestly from a table;
they need the first stages and a look at real images. The criteria, to write down before the choice:

1. **Middle grey** maps where sRGB puts a photographed 18 % grey card (about 0.46 of the display range),
   so that the neutral look does not shift the brightness of an exposed frame.
2. **No clipping at either end for an unclipped scene**: the share of pixels that reach 0 or 1 after the
   tone map, measured on the fourteen samples that decode (note 005 §3.3, §3.3b) at their as-shot exposure, against the same share for the linear
   look, which is the baseline.
3. **Monotonic and smooth**: a test on the formula (the derivative does not change sign, and does not
   jump at the toe and shoulder joins), not on images.
4. **Reference images** on the samples, reviewed by Patrick on his Linux and Windows screens (the plan's
   check), each stored as a golden render of the neutral look.

## 7. What this changes elsewhere

| Where | What | Who |
| --- | --- | --- |
| `plugin-api`, `Placement` | `after` and `before` name **operations**, and apply within the stage (§3.4); the doc comment says they name stages today. The field types do not change, the meaning does. A constraint naming an operation of another stage is an error at load time | Alice, in D-142's layer 1 (WP13): done, the validation of the cross-stage case waits for `develop` (note 005 §5.3) |
| `plugin-api`, constants | The **eight stage identifiers** and the **five data-space names**, documented with their meaning (§3.2); a test checks that `pipeline`'s definition equals them | Alice (WP13), Charlie (the test): done, `definition::tests::the_stage_and_space_names_are_the_ones_plugin_api_publishes` |
| D-142's data spaces | Three become five (§3.2), each a refinement of one of D-142's, with the three as their families (Alice's D-146 amends D-142) | Alice |
| `RawImage`'s tagged block (D-141) | Carries **`iso` and the DNG `NoiseProfile` when the file has them** (D-146 amends D-141), for the denoiser's noise model (§4.5, item 4) | Alice, in WP13 |
| The denoiser's declaration (layer 1) | Strength **in noise units** and a **noise model** `(a, b)` per channel as an input that is not a slider; settled with the denoiser's choice (plan §6, item 8) | Alice and Charlie, at that choice |
| The counting-cache test | A white balance change reruns `input-colour` and later stages and nothing earlier (§3.4) | WP14, Charlie |
| The recipe | Carries the definition version; the spine is not in it (§3.1) | already D-140; this note says what the version names |
| `develop` and the sidecar | Records the definition version with each version, next to each operation's version | WP17, note 007 |
| `pipeline` | The definition as data, the validation of §3.5, the fingerprint test of §3.6 | WP14, Charlie |
| Spike 1's report | Its sentence "the result is the same" holds for the linear steps only (§2.2); the report could say so | Alice, if she wishes |

## 8. Open points and what is needed

- **Proposed by evidence**: white balance after the denoiser (§4); it only needs re-checking if the chosen
  denoiser has no per-channel noise model. **Not yet measured**: the working space (§5), and how much a
  wrong or estimated noise profile costs a noise-model denoiser (§4.5). v1 does not freeze before the first
  two are settled.
- **Decided by looking, not yet taken**: the base look's curve and constants (§6).
- **Before v1 freezes** (the start of increment B, §3.6), besides the two measurements above: the colour
  stages' matrices must be derived from the definition's working space and from no other copy of its
  chromaticities (done: `colour.rs` reads `definition::V1`, and a test reads the chromaticities back from
  its matrix); and whoever loads the registry (WP18) must pass plugin declarations in a machine-independent
  order, since the first of two declarations of one identifier is the one kept (the doc of
  `OperationRegistry::load`).
- **Accepted in review** (D-146): the five data spaces, `after` and `before` naming operations within
  a stage, `decode` and `encode` as boundaries, the fingerprint test of released definitions, the definition
  as Rust data, the flat linear look as an absent `tone map`. **Changed after review**: stage names
  (`camera-rgb`, `input-colour`), `geometry` before `detail`, stage and space names as constants in
  `plugin-api`, the spine in the fingerprint, the cross-stage constraint error, the default recipe holding
  the neutral `tone map`, and the third arm of the experiment. **Added in the review of the pull request**:
  the rule of what an operation may depend on, with highlight reconstruction moved after white balance
  (§3.4), and the `iso` and `NoiseProfile` tags of D-141.
- **For Alice**: the denoiser's declaration and `RawImage`'s missing ISO or noise profile (§4.5, item 4);
  the log-axis curve in `scene-linear` (§6), which she flagged.
- **Not covered**: the file syntax of a configurable definition (§3.7), and how a definition version
  interacts with a **style** that stores operation instances (styles and a definition are both in the
  sidecar's vocabulary; note 007).
- **What I would do next**, each its own pull request: the definition as data with the validation and
  the fingerprint test (WP14, no shader needed); the first shaders with their references; the
  working-space measurement of §5 once there is a camera-to-working stage to
  measure.
