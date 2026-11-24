//! Which Vello backend the desktop draws with: the shared chooser, fed the facts of a computer.

use fusuma::platform::{Backend, Hardware, Machine, Plan, Render, RenderLayer};

/// The backends this crate links.
pub const LINKED: &[Backend] = &[Backend::VelloCpu, Backend::VelloGpu];

/// What a computer has, as far as the choice cares: memory is not a constraint (a frame is a few megabytes),
/// and `gpu` says whether `wgpu` found an adapter.
pub fn hardware(gpu: bool) -> Hardware {
    Hardware {
        ram_kb: u32::MAX / 2,
        psram_kb: 0,
        fpu: true,
        gpu,
    }
}

/// The plan for a window of `width` x `height`: the settings (`fusuma.toml`, then the build's layer) over the
/// automatic rule. `None` only for a window of no size.
pub fn plan_for(width: u32, height: u32, gpu: bool, layers: &[RenderLayer]) -> Option<Plan> {
    let machine = Machine {
        screen: (width > 0 && height > 0).then_some((width, height)),
        hardware: Some(hardware(gpu)),
        render: Render::layered(layers),
    };
    fusuma::platform::plan(&machine, None, LINKED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma::platform::BackendSetting;

    fn pin(b: Backend) -> RenderLayer {
        RenderLayer {
            backend: Some(BackendSetting::Fixed(b)),
            ..RenderLayer::EMPTY
        }
    }

    #[test]
    fn a_computer_with_a_gpu_draws_on_it_and_one_without_draws_on_the_cpu() {
        assert_eq!(
            plan_for(800, 600, true, &[]).unwrap().uses,
            Backend::VelloGpu
        );
        assert_eq!(
            plan_for(800, 600, false, &[]).unwrap().uses,
            Backend::VelloCpu
        );
    }

    #[test]
    fn the_layers_override_the_hardware_and_the_build_beats_the_file() {
        assert_eq!(
            plan_for(800, 600, true, &[pin(Backend::VelloCpu)])
                .unwrap()
                .uses,
            Backend::VelloCpu
        );
        let layers = [pin(Backend::VelloGpu), pin(Backend::VelloCpu)];
        assert_eq!(
            plan_for(800, 600, true, &layers).unwrap().uses,
            Backend::VelloCpu
        );
    }

    #[test]
    fn a_window_of_no_size_has_no_plan_and_memory_never_refuses_a_desktop() {
        assert!(plan_for(0, 600, true, &[]).is_none());
        let p = plan_for(16_000, 16_000, false, &[]).unwrap();
        assert_eq!(p.fit, fusuma::platform::Fit::Fits);
    }

    #[test]
    fn a_backend_the_desktop_does_not_link_is_replaced_and_the_plan_says_so() {
        let p = plan_for(800, 600, true, &[pin(Backend::Skia)]).unwrap();
        assert_eq!(
            (p.choice.backend, p.uses),
            (Backend::Skia, Backend::VelloCpu)
        );
        assert!(
            p.to_string()
                .contains("skia is not built in, using vello-cpu"),
            "{p}"
        );
    }
}
