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
