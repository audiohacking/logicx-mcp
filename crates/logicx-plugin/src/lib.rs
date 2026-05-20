mod editor;
mod plugin_state;

use std::sync::Arc;
use truce::prelude::*;
use truce_core::custom_state::State as StateTrait;

pub struct LogicxMcp {
    params: Arc<LogicxMcpParams>,
    state: plugin_state::PluginState,
}

impl LogicxMcp {
    pub fn new(params: Arc<LogicxMcpParams>) -> Self {
        Self {
            params,
            state: plugin_state::PluginState::default(),
        }
    }
}

impl PluginLogic for LogicxMcp {
    fn reset(&mut self, sample_rate: f64, _max_block_size: usize) {
        self.params.set_sample_rate(sample_rate);
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventList,
        _context: &mut ProcessContext,
    ) -> ProcessStatus {
        for ch in 0..buffer.channels() {
            let (inp, out) = buffer.io(ch);
            out.copy_from_slice(inp);
        }
        ProcessStatus::Normal
    }

    fn save_state(&self) -> Vec<u8> {
        self.state.serialize()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), truce_core::state::StateLoadError> {
        match plugin_state::PluginState::deserialize(data) {
            Some(s) => {
                self.state = s;
                Ok(())
            }
            None => Err(truce_core::state::StateLoadError::Malformed("PluginState")),
        }
    }

    fn custom_editor(&self) -> Option<Box<dyn Editor>> {
        Some(editor::build_editor(self.params.clone()))
    }
}

#[derive(Params)]
pub struct LogicxMcpParams {
    #[param(name = "Bypass", default = 0)]
    pub bypass: BoolParam,
}

truce::plugin! {
    logic: LogicxMcp,
    params: LogicxMcpParams,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_is_valid() {
        truce_test::assert_valid_info::<Plugin>();
    }

    #[test]
    fn has_editor() {
        truce_test::assert_has_editor::<Plugin>();
    }
}
