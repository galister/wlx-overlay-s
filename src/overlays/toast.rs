use std::sync::Arc;

pub struct Toast {
    pub title: Arc<str>,
    pub body: Arc<str>,
    pub opacity: f32,
    pub timeout: f32,
    pub sound: bool,
}

#[allow(dead_code)]
impl Toast {
    pub fn new(title: Arc<str>, body: Arc<str>) -> Self {
        Toast {
            title,
            body,
            opacity: 1.0,
            timeout: 3.0,
            sound: false,
        }
    }
    pub fn with_timeout(mut self, timeout: f32) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn with_sound(mut self) -> Self {
        self.sound = true;
        self
    }
}
