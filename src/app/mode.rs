//! Mutually exclusive canvas modes and their shared transition cleanup.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum CanvasMode {
    #[default]
    Select,
    Wire,
    Drag,
}
impl CanvasMode {
    fn status(self) -> &'static str {
        match self {
            Self::Select => {
                "Select mode · drag empty canvas to box select · D pans · Shift extends selection"
            }
            Self::Wire => "Wire mode · click pins, joints, or empty canvas · Esc selects",
            Self::Drag => "Drag mode · drag anywhere to pan · V or Esc selects",
        }
    }
}
impl Editor {
    pub(super) fn set_mode(&mut self, mode: CanvasMode) {
        self.finish_edit();
        self.finish_drag();
        self.drag = None;
        self.mode = mode;
        self.wire_gesture = canvas::WireGesture::default();
        if self.circuit().fsm.is_some() {
            self.fsm_ui.tool = match mode {
                CanvasMode::Wire => fsm::Tool::Transition,
                CanvasMode::Select | CanvasMode::Drag => fsm::Tool::Select,
            };
            self.fsm_ui.from = None;
        }
        self.placing = None;
        self.pending_pin = None;
        self.pending_joint = None;
        self.status = mode.status().into();
    }
    pub(super) fn toggle_mode(&mut self, mode: CanvasMode) {
        self.set_mode(if self.mode == mode {
            CanvasMode::Select
        } else {
            mode
        });
    }
}
