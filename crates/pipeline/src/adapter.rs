// SPDX-License-Identifier: GPL-3.0-or-later
//! Which graphics adapter the engine runs on (architecture §6.6; design note 005 §6).
//!
//! The rules: only Vulkan, Metal and DirectX 12 (wgpu's OpenGL back end lost the device in spike 1
//! and is not a target); the best real GPU first (discrete, then integrated), a software adapter
//! only when nothing else exists or when it is asked for by name; a setting can override the choice.
//! The choice itself ([`choose`]) is a pure function of a list of descriptions, so it is tested
//! without a graphics adapter.

use std::fmt;

use wgpu::Backends;

/// A graphics API the engine uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Vulkan (Linux, and Windows as an alternative).
    Vulkan,
    /// Metal (macOS).
    Metal,
    /// DirectX 12 (Windows).
    Dx12,
}

impl Backend {
    /// The graphics API of this platform (architecture §12: Vulkan on Linux, DirectX 12 on Windows,
    /// Metal on macOS). When a GPU is reachable through several APIs, this is the one used, so that
    /// the same photo does not render through different drivers depending on the order the APIs
    /// list adapters in.
    pub const fn native() -> Backend {
        if cfg!(target_os = "windows") {
            Backend::Dx12
        } else if cfg!(target_os = "macos") {
            Backend::Metal
        } else {
            Backend::Vulkan
        }
    }

    /// The words [`AdapterChoice::Named`] finds this back end by: the ones a person types (`dx12`)
    /// and the ones it is displayed with (`DirectX 12`).
    const fn keywords(self) -> &'static str {
        match self {
            Backend::Vulkan => "vulkan",
            Backend::Metal => "metal",
            Backend::Dx12 => "dx12 directx 12",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Backend::Vulkan => "Vulkan",
            Backend::Metal => "Metal",
            Backend::Dx12 => "DirectX 12",
        })
    }
}

/// What kind of device an adapter is. The interface says so when the engine runs on a software
/// adapter, and the render report carries it (design note 005 §2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterKind {
    /// A discrete graphics card.
    Discrete,
    /// A GPU integrated with the processor.
    Integrated,
    /// A GPU the driver does not classify.
    Other,
    /// A virtual GPU (a virtual machine, a continuous-integration runner).
    Virtual,
    /// A software rasteriser on the CPU (llvmpipe, WARP): the fallback (architecture §6.6).
    Software,
}

impl AdapterKind {
    /// Lower is preferred by [`AdapterChoice::Best`].
    fn rank(self) -> u8 {
        match self {
            AdapterKind::Discrete => 0,
            AdapterKind::Integrated => 1,
            AdapterKind::Other => 2,
            AdapterKind::Virtual => 3,
            AdapterKind::Software => 4,
        }
    }
}

/// What the engine knows about an adapter, without holding a device on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    /// The adapter's name as the driver reports it.
    pub name: String,
    /// The graphics API it is reached through.
    pub backend: Backend,
    /// What kind of device it is.
    pub kind: AdapterKind,
    /// The driver's name.
    pub driver: String,
    /// The driver's version string.
    pub driver_info: String,
    /// The largest buffer the adapter can create, in bytes.
    pub max_buffer_size: u64,
    /// The largest range of a storage buffer a shader can bind, in bytes: the limit that decides
    /// the height of a band (architecture §6.3; 128 MiB on llvmpipe).
    pub max_storage_binding_size: u64,
}

impl AdapterInfo {
    /// The text [`AdapterChoice::Named`] matches against: the back end's keywords and the name,
    /// lowercase. `dx12` and `directx 12` both find a DirectX 12 adapter.
    fn searchable(&self) -> String {
        format!("{} {}", self.backend.keywords(), self.name).to_lowercase()
    }

    /// Where this adapter stands when several could do, on a platform whose own API is `native`:
    /// the kind first (a discrete card over an integrated one, whatever the API), then the native
    /// API over the others. Lower is better.
    fn preference(&self, native: Backend) -> (u8, u8) {
        (self.kind.rank(), u8::from(self.backend != native))
    }

    /// A short description for logs and reports, such as `NVIDIA GeForce GTX 1650 SUPER (Vulkan, discrete)`.
    pub fn describe(&self) -> String {
        let kind = match self.kind {
            AdapterKind::Discrete => "discrete",
            AdapterKind::Integrated => "integrated",
            AdapterKind::Other => "unclassified",
            AdapterKind::Virtual => "virtual",
            AdapterKind::Software => "software",
        };
        format!("{} ({}, {kind})", self.name, self.backend)
    }
}

/// Which adapter to run on: the setting of architecture §6.6.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AdapterChoice {
    /// The best real GPU (discrete, then integrated), and a software adapter only if there is none.
    #[default]
    Best,
    /// The software adapter, for tests that must not depend on the machine's GPU (continuous
    /// integration) and for a person who wants it.
    Software,
    /// The best adapter (in [`AdapterChoice::Best`]'s order) whose back end and name contain every
    /// word given, case-insensitively: `"vulkan nvidia"`, `"dx12"` or `"directx 12"`, `"llvmpipe"`.
    Named(String),
}

/// Why no adapter could be chosen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChooseError {
    /// There is no graphics adapter at all (a virtual machine, some remote sessions).
    #[error("no graphics adapter is available (the engine needs Vulkan, Metal or DirectX 12)")]
    NoAdapter,
    /// The setting names no adapter that exists.
    #[error("no adapter matches {wanted:?}; the adapters are: {available}")]
    NoMatch {
        /// The words asked for.
        wanted: String,
        /// The adapters that exist, as [`AdapterInfo::describe`] gives them.
        available: String,
    },
}

/// Picks the adapter to use among `adapters` and returns its index, for this platform's own API
/// ([`Backend::native`]).
pub fn choose(adapters: &[AdapterInfo], choice: &AdapterChoice) -> Result<usize, ChooseError> {
    choose_for(adapters, choice, Backend::native())
}

/// [`choose`] for a platform whose own graphics API is `native`, so that the tie-break between
/// the APIs that reach one GPU is stated and tested on every platform, not only the one running.
///
/// Among the adapters the choice allows, the best by [`AdapterInfo::preference`] wins, and of two
/// equal ones the first in `adapters`: the result does not depend on the order of the list, except
/// to break a true tie.
pub fn choose_for(
    adapters: &[AdapterInfo],
    choice: &AdapterChoice,
    native: Backend,
) -> Result<usize, ChooseError> {
    if adapters.is_empty() {
        return Err(ChooseError::NoAdapter);
    }
    let words: Vec<String> = match choice {
        AdapterChoice::Named(words) => words
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    };
    let found = adapters
        .iter()
        .enumerate()
        .filter(|(_, a)| match choice {
            AdapterChoice::Best => true,
            AdapterChoice::Software => a.kind == AdapterKind::Software,
            AdapterChoice::Named(_) => {
                let text = a.searchable();
                !words.is_empty() && words.iter().all(|w| text.contains(w))
            }
        })
        .min_by_key(|(index, a)| (a.preference(native), *index))
        .map(|(index, _)| index);
    found.ok_or_else(|| ChooseError::NoMatch {
        wanted: match choice {
            AdapterChoice::Software => "software".to_string(),
            AdapterChoice::Named(words) => words.clone(),
            AdapterChoice::Best => String::new(),
        },
        available: adapters
            .iter()
            .map(AdapterInfo::describe)
            .collect::<Vec<_>>()
            .join(", "),
    })
}

/// The adapter descriptions, best first: discrete, integrated, unclassified, virtual, software;
/// within a kind, this platform's own API first ([`Backend::native`]), then the order the graphics
/// API lists them in.
pub fn list_adapters() -> Vec<AdapterInfo> {
    enumerate().into_iter().map(|(_, info)| info).collect()
}

/// The one graphics-API instance of the process.
///
/// Created once and never dropped: creating and destroying Vulkan instances from several threads at
/// the same time crashed the process (a segmentation fault, 14 runs in 15, with two adapters on one
/// machine), and there is no reason to have more than one. Every adapter and device comes from it.
fn instance() -> &'static wgpu::Instance {
    static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
    INSTANCE.get_or_init(wgpu::Instance::default)
}

/// The adapters wgpu offers with their descriptions, best first.
pub(crate) fn enumerate() -> Vec<(wgpu::Adapter, AdapterInfo)> {
    let instance = instance();
    let backends = Backends::VULKAN | Backends::METAL | Backends::DX12;
    let mut adapters: Vec<(wgpu::Adapter, AdapterInfo)> =
        pollster::block_on(instance.enumerate_adapters(backends))
            .into_iter()
            .filter_map(|adapter| {
                let info = describe(&adapter)?;
                Some((adapter, info))
            })
            .collect();
    // A stable sort: adapters that tie keep the order the API gave them.
    adapters.sort_by_key(|(_, info)| info.preference(Backend::native()));
    adapters
}

/// Describes an adapter, or `None` for a back end the engine does not use.
fn describe(adapter: &wgpu::Adapter) -> Option<AdapterInfo> {
    let info = adapter.get_info();
    let backend = match info.backend {
        wgpu::Backend::Vulkan => Backend::Vulkan,
        wgpu::Backend::Metal => Backend::Metal,
        wgpu::Backend::Dx12 => Backend::Dx12,
        _ => return None,
    };
    let kind = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => AdapterKind::Discrete,
        wgpu::DeviceType::IntegratedGpu => AdapterKind::Integrated,
        wgpu::DeviceType::VirtualGpu => AdapterKind::Virtual,
        wgpu::DeviceType::Cpu => AdapterKind::Software,
        wgpu::DeviceType::Other => AdapterKind::Other,
    };
    let limits = adapter.limits();
    Some(AdapterInfo {
        name: info.name,
        backend,
        kind,
        driver: info.driver,
        driver_info: info.driver_info,
        max_buffer_size: limits.max_buffer_size,
        max_storage_binding_size: limits.max_storage_buffer_binding_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, backend: Backend, kind: AdapterKind) -> AdapterInfo {
        AdapterInfo {
            name: name.to_string(),
            backend,
            kind,
            driver: String::new(),
            driver_info: String::new(),
            max_buffer_size: 1 << 30,
            max_storage_binding_size: 1 << 27,
        }
    }

    /// The machine of spike 1's reference runs: one discrete card, one software adapter.
    fn desk() -> Vec<AdapterInfo> {
        vec![
            adapter(
                "NVIDIA GeForce GTX 1650 SUPER",
                Backend::Vulkan,
                AdapterKind::Discrete,
            ),
            adapter(
                "llvmpipe (LLVM 20.1.2, 256 bits)",
                Backend::Vulkan,
                AdapterKind::Software,
            ),
        ]
    }

    /// A Windows machine: one discrete card reached through both DirectX 12 and Vulkan, an
    /// integrated GPU, and WARP. The Vulkan listing comes first, as it can.
    fn windows_desk() -> Vec<AdapterInfo> {
        vec![
            adapter(
                "NVIDIA GeForce GTX 1650 SUPER",
                Backend::Vulkan,
                AdapterKind::Discrete,
            ),
            adapter(
                "NVIDIA GeForce GTX 1650 SUPER",
                Backend::Dx12,
                AdapterKind::Discrete,
            ),
            adapter(
                "Intel(R) UHD Graphics 630",
                Backend::Dx12,
                AdapterKind::Integrated,
            ),
            adapter(
                "Microsoft Basic Render Driver",
                Backend::Dx12,
                AdapterKind::Software,
            ),
        ]
    }

    #[test]
    fn dx12_and_directx_12_both_find_a_directx_adapter() {
        let adapters = windows_desk();
        for words in [
            "dx12",
            "DX12 nvidia",
            "directx 12",
            "DirectX 12 intel",
            "dx12 warp",
        ] {
            let found = choose_for(
                &adapters,
                &AdapterChoice::Named(words.into()),
                Backend::Dx12,
            );
            match words {
                "dx12 warp" => assert!(found.is_err(), "WARP is called Basic Render Driver"),
                _ => assert!(
                    found.is_ok_and(|i| adapters[i].backend == Backend::Dx12),
                    "{words:?} must find a DirectX 12 adapter"
                ),
            }
        }
    }

    #[test]
    fn vulkan_and_metal_are_found_by_their_names() {
        let adapters = windows_desk();
        assert_eq!(
            choose_for(
                &adapters,
                &AdapterChoice::Named("vulkan".into()),
                Backend::Dx12
            ),
            Ok(0)
        );
        let mac = vec![adapter("Apple M2", Backend::Metal, AdapterKind::Integrated)];
        assert_eq!(
            choose_for(
                &mac,
                &AdapterChoice::Named("metal apple".into()),
                Backend::Metal
            ),
            Ok(0)
        );
    }

    #[test]
    fn a_gpu_listed_under_two_apis_is_taken_through_the_platforms_own() {
        let windows = windows_desk();
        // DirectX 12 on Windows, although Vulkan is listed first...
        assert_eq!(
            choose_for(&windows, &AdapterChoice::Best, Backend::Dx12),
            Ok(1)
        );
        // ...and whichever order the API lists them in.
        let mut reversed = windows.clone();
        reversed.swap(0, 1);
        assert_eq!(
            choose_for(&reversed, &AdapterChoice::Best, Backend::Dx12),
            Ok(0)
        );
        assert_eq!(reversed[0].backend, Backend::Dx12);
        // On Linux the same two entries give Vulkan.
        assert_eq!(
            choose_for(&windows, &AdapterChoice::Best, Backend::Vulkan),
            Ok(0)
        );
    }

    #[test]
    fn the_kind_outranks_the_platforms_own_api() {
        // A discrete card through Vulkan beats an integrated GPU through the native DirectX 12.
        let adapters = vec![
            adapter("Intel UHD", Backend::Dx12, AdapterKind::Integrated),
            adapter("NVIDIA", Backend::Vulkan, AdapterKind::Discrete),
        ];
        assert_eq!(
            choose_for(&adapters, &AdapterChoice::Best, Backend::Dx12),
            Ok(1)
        );
    }

    #[test]
    fn the_choice_does_not_depend_on_the_order_of_the_list_except_to_break_a_tie() {
        let forward = windows_desk();
        let mut backward = forward.clone();
        backward.reverse();
        let pick = |list: &[AdapterInfo]| {
            let i = choose_for(list, &AdapterChoice::Best, Backend::Dx12).unwrap();
            (list[i].name.clone(), list[i].backend)
        };
        assert_eq!(pick(&forward), pick(&backward));
    }

    #[test]
    fn the_native_backend_is_the_one_of_the_platform() {
        let native = Backend::native();
        if cfg!(target_os = "windows") {
            assert_eq!(native, Backend::Dx12);
        } else if cfg!(target_os = "macos") {
            assert_eq!(native, Backend::Metal);
        } else {
            assert_eq!(native, Backend::Vulkan);
        }
    }

    #[test]
    fn no_adapter_at_all_is_a_typed_error_for_every_choice() {
        for choice in [
            AdapterChoice::Best,
            AdapterChoice::Software,
            AdapterChoice::Named("vulkan".into()),
        ] {
            assert_eq!(choose(&[], &choice), Err(ChooseError::NoAdapter));
        }
    }

    #[test]
    fn best_is_the_first_of_the_list() {
        assert_eq!(choose(&desk(), &AdapterChoice::Best), Ok(0));
    }

    #[test]
    fn best_falls_back_to_the_software_adapter_when_it_is_the_only_one() {
        let only = vec![adapter("WARP", Backend::Dx12, AdapterKind::Software)];
        assert_eq!(choose(&only, &AdapterChoice::Best), Ok(0));
    }

    #[test]
    fn software_finds_the_software_adapter_and_says_when_there_is_none() {
        assert_eq!(choose(&desk(), &AdapterChoice::Software), Ok(1));
        let gpu_only = vec![adapter("Apple M2", Backend::Metal, AdapterKind::Integrated)];
        assert!(matches!(
            choose(&gpu_only, &AdapterChoice::Software),
            Err(ChooseError::NoMatch { .. })
        ));
    }

    #[test]
    fn named_needs_every_word_and_ignores_case() {
        let adapters = desk();
        assert_eq!(
            choose(&adapters, &AdapterChoice::Named("VULKAN nvidia".into())),
            Ok(0)
        );
        assert_eq!(
            choose(&adapters, &AdapterChoice::Named("llvmpipe".into())),
            Ok(1)
        );
        assert!(matches!(
            choose(&adapters, &AdapterChoice::Named("dx12 nvidia".into())),
            Err(ChooseError::NoMatch { .. })
        ));
    }

    #[test]
    fn an_empty_name_matches_nothing_instead_of_everything() {
        assert!(matches!(
            choose(&desk(), &AdapterChoice::Named("  ".into())),
            Err(ChooseError::NoMatch { .. })
        ));
    }

    #[test]
    fn a_no_match_error_lists_the_adapters_that_exist() {
        let Err(ChooseError::NoMatch { available, .. }) =
            choose(&desk(), &AdapterChoice::Named("metal".into()))
        else {
            panic!("expected NoMatch");
        };
        assert!(available.contains("GTX 1650 SUPER"), "{available}");
        assert!(available.contains("software"), "{available}");
    }

    #[test]
    fn kinds_rank_real_gpus_before_virtual_and_software_ones() {
        let order = [
            AdapterKind::Discrete,
            AdapterKind::Integrated,
            AdapterKind::Other,
            AdapterKind::Virtual,
            AdapterKind::Software,
        ];
        assert!(order.windows(2).all(|w| w[0].rank() < w[1].rank()));
    }
}
