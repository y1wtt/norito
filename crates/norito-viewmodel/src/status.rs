//! The one-line progress or error message.

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub message: String,
    pub error: bool,
}

impl Status {
    pub fn fail(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.error = true;
    }
    pub fn note(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.error = false;
    }
    pub fn clear(&mut self) {
        self.note("");
    }
}
