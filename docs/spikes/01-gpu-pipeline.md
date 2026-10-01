# Spike 1: the GPU pipeline (interim report)

> **Status: interim.** Measured on Linux with a real GPU, on real RAW files from six cameras,
> and on Linux, Windows and macOS through continuous integration (no discrete GPU there, and
> synthetic data), and with a heavier pipeline (denoising, sharpening, local contrast, a mask).
> A real Windows GPU has been measured on the same machine, on **Vulkan and DirectX 12**, and on
> the **Intel UHD 630** as well as the NVIDIA card. A Mac and the sRAW entry point are still to
> come (see "What remains"). Nothing here is a final decision.

## Question

Can one Rust pipeline on wgpu turn a RAW into a colour-managed image fast enough, with the same
result on Vulkan, Metal and DirectX 12, and with a CPU fallback? (docs/technical-spikes.md, §2)

## What was built

`spikes/gpu-pipeline` (at the tag `spikes-final`), about 700 lines of Rust and WGSL:

- Three compute shaders: **demosaicing** (Malvar-He-Cutler gradient-corrected interpolation,
  with black level and white balance), **box downscale**, and **tone** (camera RGB to a wide
  gamut working space, exposure, a tone curve, working space to display primaries, a 33x33x33
  LUT standing in for an ICC display profile, sRGB encoding, 8-bit packing).
- A **CPU reference** with the same maths in f32, multi-threaded (rayon).
- Three paths measured: a **full-resolution export** by full-width bands, a **100% view** (a
  2560x1440 viewport), and a **fit-to-screen view**, each with the camera RGB cached at its
  stage boundary so that a downstream change reruns only the tone pass.
- A benchmark that prints every measure as JSON.

The first input was a synthetic Bayer mosaic (smooth colour field, texture, noise). The second
round, below, uses **real RAW files**, decoded with `rawler` (Rust, LGPL-2.1): black and white
levels, white balance, the camera's colour matrix and the recommended crop come from the file.
A generic demosaicing pass (a weighted average over a 7x7 window, for any 6x6 colour filter
pattern) was added to show that the demosaicing stage can be swapped, using Fujifilm's X-Trans.

## Results

Machine: NVIDIA GeForce GTX 1650 SUPER (4 GB, Vulkan, driver 580.178), 16 CPU threads. The
software adapter is llvmpipe (Mesa 25.2, LLVM 20), which runs the very same shaders on the CPU.

| Measure (target) | GTX 1650 SUPER, 24 MP | GTX 1650 SUPER, 61 MP | llvmpipe, 24 MP |
| --- | --- | --- | --- |
| Full render, developed and tone-mapped | **27 ms** | **62 ms** | 235 ms |
| Read back to the CPU (96 MB / 241 MB) | 35 ms | 86 ms | 33 ms |
| 100% view, upstream change (demosaic and tone) | **1.2 ms** | 1.2 ms | 23.9 ms |
| 100% view, downstream change (tone only) (target: under 50 ms) | **0.6 ms** | 0.6 ms | 14.7 ms |
| Fit view, cold | 12 ms | 36 ms | 154 ms |
| Fit view, slider (target: under 50 ms) | **0.4 ms** | 0.6 ms | 11.9 ms |
| Accuracy against the CPU reference: worst difference | 1 level | 1 level | 1 level |
| Pixels differing by more than 1 level (target: under 0.1%) | 0% | 0% | 0% |
| CPU reference, 16 threads | 354 ms | 966 ms | 401 ms |

### The three platforms, through continuous integration

The GitHub runners have no discrete GPU, so these are the adapters they offer. They test that
the shaders run and agree, not that a real GPU is fast. All four have 3 to 4 CPU threads.

| Adapter | Full render, 24 MP | 100% view, tone only | Fit view, slider | Worst difference vs CPU | Rust CPU reference |
| --- | --- | --- | --- | --- | --- |
| macOS, **Metal**, Apple paravirtual GPU | 89 ms | 2.0 ms | 1.8 ms | 1 level | 1,094 ms |
| Linux, **Vulkan**, llvmpipe (software) | 677 ms | 57.9 ms | 42.7 ms | 1 level | 1,288 ms |
| Windows, **DirectX 12**, WARP (software) | 3,056 ms | 122.2 ms | 88.4 ms | 1 level | 1,130 ms |

In every case the fraction of pixels differing by more than one level is 0%.

### Real RAW files

Seven CC0 samples from raw.pixls.us. Same GTX 1650 SUPER, Vulkan. "Decode" is the CPU time
`rawler` needs to read the file; the other columns are GPU. "Worst diff" is the largest
difference from the CPU reference, over the whole image; no pixel differs by more than 1 level.

| File | Sensor | Size | Decode | Full render | 100% view, upstream | 100% view, tone only | Worst diff |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Sony A7R IV (ARW) | Bayer | 60 MP | 115 ms | 63 ms | 1.1 ms | 0.6 ms | 1 level |
| Nikon D850 (NEF) | Bayer | 45 MP | 301 ms | 51 ms | 1.4 ms | 0.9 ms | 1 level |
| Canon R5 Mark II (CR3, compressed) | Bayer | 45 MP | 378 ms | 45 ms | 1.5 ms | 0.8 ms | 1 level |
| Fujifilm X-T50 (RAF) | **X-Trans** | 40 MP | 304 ms | 90 ms | 6.0 ms | 1.3 ms | 1 level |
| Panasonic S5 (RW2) | Bayer | 24 MP | 54 ms | 25 ms | 1.2 ms | 0.7 ms | 1 level |
| Olympus E-M5 III (ORF) | Bayer | 20 MP | 291 ms | 24 ms | 1.4 ms | 0.9 ms | 1 level |
| Canon 5D Mark IV (CR2) | **sRAW**, 3 channels | 7.5 MP | not run | not run | not run | not run | not run |

The previews written by the benchmark were checked by eye: correct orientation, plausible
colours, no visible pattern from the demosaicing, on Sony, Nikon, Canon and Fujifilm.

Other facts:

- **Memory.** The 61 MP run used about 700 MB of GPU memory in the process. The desktop was
  already using about 2.3 GB of the 4 GB card.
- **Small binding limit.** llvmpipe limits a storage binding to 128 MiB, which forced the
  band size down (1,398 rows at 24 MP). The banded design worked there without change, so it
  will fit any GPU.
- **wgpu's GL back end** lost the device on this workload. It is not a target platform.
- **Intel UHD 630** is not visible to wgpu from this sandbox, so it could not be tested.

## Second round: a heavier pipeline

The minimal pipeline left a lot of headroom, so the question was where it runs out. Four
operations were added after demosaicing, in this order, on the camera RGB:

1. **Non-local means denoising**, search window 11x11, 3x3 patches. The most expensive
   operation in most RAW developers, and the one that scales worst.
2. **Unsharp mask** (box blur radius 2).
3. **Local contrast** (box blur radius 24).
4. **A radial exposure mask.**

Each stage has its own buffer, so a change reruns its stage and every later one, and the earlier
results stay cached. The work is done on a region with a **40 px halo** so the edges of the view
are right. A CPU version of the whole chain checks the shaders: on the GTX 1650 SUPER the two
agree exactly on the 256x256 test region, on llvmpipe within one level.

Nikon D850 file, 45 MP, GTX 1650 SUPER, a **2560x1440 view at 100%** (3.7 MP). The Sony 60 MP file
gives the same numbers within 2%.

| Stage alone | Time |
| --- | --- |
| Demosaic | 1.0 ms |
| **Denoise (non-local means)** | **108 ms** |
| Blurs (four passes) | 9.6 ms |
| Sharpen, local contrast and mask | 2.1 ms |
| Tone and display | 1.0 ms |

| What the photographer changes (target: under 50 ms) | Time | Stages rerun |
| --- | --- | --- |
| Exposure, tone curve | **0.7 ms** | tone |
| Local contrast amount, sharpening amount, mask | **2.6 ms** | combine, tone |
| Sharpening radius | 11 ms | blurs and later |
| **Denoise strength** | **118 ms** | denoise and later |
| **White balance, applied before denoising** | **120 ms** | everything |
| **White balance, applied after denoising** | **0.6 ms** | tone |

How the denoise cost grows, on the same view (a search window of (2R+1)^2 pixels):

| Search radius | 2 | 3 | 5 | 7 | 9 |
| --- | --- | --- | --- | --- | --- |
| Denoise change on 2560x1440 | 32 ms | 52 ms | 120 ms | 237 ms | 419 ms |

And how it grows with the view, at radius 5: 1280x720 (0.9 MP) 31 ms, 1920x1080 (2.1 MP) 69 ms,
2560x1440 (3.7 MP) 120 ms. A 3840x2160 view did not fit in the free GPU memory.

| Other paths | GTX 1650 SUPER | llvmpipe (16 threads) |
| --- | --- | --- |
| Fit-to-screen, denoise change (radii scaled by 1/4) | 19 ms | 542 ms |
| Fit-to-screen, local contrast change | 1.7 ms | 26 ms |
| Full export through the chain, 45 MP | **2.0 s** | 55.7 s |
| Full export through the chain, 60 MP | 2.9 s | not run |
| The 100% view, denoise change | 118 ms | 3,233 ms |

The export used small bands (150 MB of buffers), so the halo was redundant work for about half
of each band. It would be faster with bigger bands.

### What it means

1. **Everything but the denoiser fits the budget by a wide margin.** Sharpening, local contrast,
   masks, exposure and tone cost under 12 ms together. The denoiser alone is 108 ms.
2. **The order of operations decides how interactive the application feels.** The same white
   balance change costs 120 ms before the denoiser and 0.6 ms after it, 200 times less, and the
   result is the same, since the multipliers fold into the camera matrix. *(Erratum, 2026-09-30: that holds for
   the linear steps only. Through the denoiser the two orders are different images, within a few tenths of a
   decibel of each other; design note 006 §2.2 and §4 measured it, and D-146 decided on the cost.)* This is also how real
   RAW developers do it, and the pipeline definition (specification §5.6) must be able to say it:
   heavy operations that are rarely changed go early, operations dragged often go late.
3. **A draft quality would keep dragging under 50 ms.** Denoising with a search radius of 2
   costs 32 ms on a 2560x1440 view, against 120 ms at radius 5. Drawing the draft while a slider
   moves and the full quality when it stops would meet the budget on this card. On a faster GPU,
   and that is a mid-range one, the full quality alone might.
4. **The fit-to-screen view is not the problem**: 19 ms with the radii scaled to the reduced
   image.
5. **The export target holds**: about 1 s per 24 MP through the whole chain, against a target of
   2 s, even with wasteful bands.
6. **The software fallback struggles with heavy operations.** llvmpipe is 27 times slower than
   the GPU on the denoiser (target: under 20 times), and a 45 MP export takes 56 s. The fallback
   is acceptable for the light pipeline and painful for the heavy one; it would need a cheaper
   denoising algorithm, or a warning.
7. **The application must adapt to the GPU memory it can get.** During this round the memory a
   process could allocate on this 4 GB card went from 1.3 GB to 64 MB and back, as the desktop
   and a virtual machine used it. One chain at 2560x1440 needs five 60 MB buffers, so the
   view size, the band size and the halo have to follow the available memory, and running out
   must be handled and not fatal. Half-precision intermediates would halve the memory.
8. **Non-local means is a placeholder for the choice of denoiser**, which is a product decision:
   its cost grows with the square of the search radius, and the AI denoiser planned for
   milestone M5 has a different profile.

### A portability lesson, found on Windows

The first Windows run of `rawbench` on Patrick's machine failed to create the generic
demosaicing pipeline. WGSL is translated to each platform's shader language, and on DirectX 12
the legacy FXC compiler rejected a construct that Vulkan and Metal accept: writing one component
of a vector through a dynamic index inside a loop (`sum[c] = ...`). Nothing in the continuous
integration had exercised that shader on Windows. The shader now uses masks instead, and a
**smoke test** compiles every shader and checks it against the CPU reference on every platform,
as a blocking step of the build. All the shaders now pass on Vulkan, Metal and DirectX 12.

This matters beyond the spike. **A shader that works on one graphics API can fail on another.**
Shaders shipped by operation plugins (specification §5.6 and §5.10) will need the same treatment:
a portable subset of WGSL that they must stay within, and a check on every back end before a
plugin is accepted into the index, because the author will usually have tried only one.

## The same machine under Windows

Patrick booted the same PC into Windows and ran `run-all.sh` (issue #1 of the repository, results
in `docs/spikes/results-windows/`). Same GTX 1650 SUPER, driver 566.24, wgpu on **Vulkan**. Windows had
many updates pending and a busy disk during the run, which could disturb timings; the figures
below are close to Linux's, so the effect on the GPU work was small.

| Measure | Linux | Windows |
| --- | --- | --- |
| Full render 24 MP, synthetic | 27 ms | 25 ms |
| 100% view, upstream / tone only | 1.2 ms / 0.6 ms | 1.2 ms / 0.7 ms |
| Real files, full render (Sony 60 MP / Nikon 45 MP) | 63 / 51 ms | 56 / 73 ms |
| Real files, decode (Sony / Nikon / Canon R5 II) | 115 / 301 / 378 ms | 103 / 285 / 514 ms |
| Heavy chain: denoise stage alone, 2560x1440 | 108 ms | 96 ms |
| Heavy chain: denoise strength change / WB after denoise | 118 / 0.7 ms | 105 / 0.6 ms |
| Heavy chain: full export, 45 MP | 2.0 s | 1.8 s |
| Heavy chain, a 3840x2160 view | did not fit in GPU memory | 232 ms |
| Worst difference from the CPU reference | 1 level | 1 level |
| Smoke test (all shaders against the CPU reference) | passes | **passes** |
| Readback of a 2 MP view | 0.9 ms | 0.75 ms |

The 4K view fitted this time because the Windows desktop leaves more GPU memory free. Nothing in
these numbers changes the conclusions above.

The Windows machine reports six adapters: the GTX 1650 SUPER on Vulkan, DirectX 12 and OpenGL,
the **Intel UHD 630 on Vulkan and DirectX 12** (it was invisible under Linux), and WARP. The
benchmarks ran on the NVIDIA card through Vulkan. **DirectX 12 on both real adapters** (issue #1,
2026-09-22) passed the smoke test: the NVIDIA card and the Intel iGPU each agree with the
reference within one 8-bit level, on 0% of pixels beyond one level, on every check (Bayer,
X-Trans, tiled Bayer, the fuller chain, the fit-to-screen pass). DirectX 12, what wgpu picks by
default on Windows, is confirmed on real hardware, discrete and integrated.

## What it means

1. **The architecture holds.** Caching the camera RGB at the stage boundary makes a downstream
   change cost about 0.6 ms at a 100% view, against a budget of 50 ms. An upstream change costs
   about 1.2 ms. A full 24 MP render is 27 ms.
2. **There is a lot of headroom, but this is a minimal pipeline.** The real one will add noise
   reduction, sharpening, local contrast, masks, lens correction and more, and heavy operations
   will eat into the headroom. The budget is safe for now, not proven for M3.
3. **Banding works** and is what makes 61 MP images fit small GPUs.
   **Real files behave like the synthetic one**: same timings, same accuracy.
3b. **Decoding is now the slow part.** Reading a RAW takes 55 to 380 ms on the CPU, against 25
   to 90 ms for the whole GPU render. Opening a photo for development therefore costs about
   0.4 s before the first pixel, whatever the GPU. Ways to hide it: show the embedded JPEG
   preview at once, decode in the background while browsing, and decode on several threads.
   `rawler` was measured here on its own; LibRaw, and LibRaw as WebAssembly, come in spike 4.
3c. **The pipeline needs several entry points.** A Bayer mosaic (fast gradient-corrected
   demosaicing), another colour filter such as X-Trans (the generic pass, 90 ms for 40 MP instead
   of about 50 ms), and **already-interpolated linear data**, which is what Canon's sRAW and
   many scanner files are. The Canon 5D Mark IV sample is the latter: three channels per pixel,
   no demosaicing to do. Only the first stage changes; the rest of the pipeline is untouched,
   which supports the pipeline definition of the specification (§5.6).
4. **The CPU fallback depends on the platform.** On Linux, the same shaders on llvmpipe were
   about 9 times slower than the GPU (target: under 20 times), gave the same result within one
   level, and were **faster** than a hand-written Rust CPU implementation (235 ms against 354 ms
   on 16 threads; 677 ms against 1,288 ms on 4). That favours the shared-shader strategy there.
   **On Windows the picture reverses:** WARP took 3,056 ms, nearly three times slower than the
   Rust CPU code on the same cores (1,130 ms), and its slider time (88 ms) misses the 50 ms
   target. macOS always has a GPU. In practice almost every Windows machine has a DirectX 12
   capable GPU, including integrated ones, so the fallback matters for old or virtual machines.
   Decision deferred: the choices are shared shaders everywhere (simple, slow on Windows
   without a GPU) or shared shaders plus a Rust CPU path used where the software adapter is too
   slow (faster, two implementations to keep equal).
5. **The four adapters agree.** NVIDIA and llvmpipe on Vulkan, Metal, and DirectX 12 all give
   the same image within one 8-bit level, on 0% of pixels beyond one level. This is the
   cross-platform result the spike was after, on the three graphics APIs.
6. **DirectX 12 holds on real Windows hardware, discrete and integrated.** The GTX 1650 SUPER and
   the Intel UHD 630, both on DirectX 12, pass the same smoke test as Vulkan and Metal did: this
   closes the item that mattered most for Windows, since DirectX 12 is wgpu's default there.

## What remains

- [x] **A real GPU on Windows** (Vulkan, same PC): matches Linux.
- [x] **DirectX 12 on a real GPU and the Intel iGPU** (issue #1, 2026-09-22): both pass the smoke
  test, agreeing with the reference within one level on 0% of pixels beyond it.
- [ ] **A Mac** with a real display.
- [x] **Real RAW files** from six cameras (done). Still open: the linear entry point (sRAW).
- [ ] **Better inputs.** The automatic exposure of the benchmark is crude and over-exposes
  bright scenes (the Canon R5 II preview clips its highlights). Black levels are averaged over
  the four channels. Highlight reconstruction, lens and chromatic-aberration corrections are
  absent. None of this affects the timings, but it limits what the previews say about quality.
- [x] **Upload time**: 70 to 160 ms for a 45 MP mosaic (91 MB) and the LUT, varying with the
  state of the GPU.
- [x] **A heavier pipeline** (done, see above). Still open: a faster GPU to see whether the
  full-quality denoiser fits on its own, and half-precision intermediates.
- [ ] **Presentation.** These numbers stop at the buffer: no window, no swap chain, no vertical
  sync. That belongs to spike 2.
- [ ] **A second GPU**, such as the Intel UHD 630, or an AMD card.
- [x] **Sensors that are not Bayer.** X-Trans works through the generic pass, at a modest
  quality. A proper X-Trans algorithm (for example Markesteijn) is a product task, not a spike.

## Running it

```bash
cd spikes
cargo build --release
./target/release/adapters                                  # lists what wgpu sees
./target/release/bench --adapter "vulkan nvidia" --mp 24    # synthetic; or 61; "dx12", "metal"
./fetch-samples.sh                                         # the seven CC0 RAW files, 225 MB
./target/release/rawbench --adapter "vulkan nvidia"        # real files; --generic forces the generic pass
./target/release/heavy --adapter "vulkan nvidia" --file samples/DSC00396.ARW   # the heavy chain
```

On Windows, run the shell script from Git Bash. `rawbench` writes previews to
`samples/previews/`.

Raw results are in `docs/spikes/results/`.
