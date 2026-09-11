#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub ts: i64,
    pub app: String,
    pub window_title: String,
    pub url: Option<String>,
    pub document_path: Option<String>,
    pub bundle_id: Option<String>,
    pub idle_seconds: i64,
    pub screen_locked: bool,
    pub paused: bool,
    pub secure_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quest {
    pub text: String,
    pub keywords: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    Away,
    Distraction,
    Side,
    CoreCandidate,
    CoreReading,
    UnsureReading,
    Unsure,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivitySeconds {
    pub core: i64,
    pub support: i64,
    pub admin: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
    pub unobserved: i64,
}

impl ActivitySeconds {
    pub fn add_assign(&mut self, other: &Self) {
        self.core += other.core;
        self.support += other.support;
        self.admin += other.admin;
        self.side += other.side;
        self.distraction += other.distraction;
        self.away += other.away;
        self.unobserved += other.unobserved;
    }
}
