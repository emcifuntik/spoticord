use log::info;

pub struct StatsManager {
    // Simple in-memory stats for now
}

impl StatsManager {
    pub fn new() -> Self {
        StatsManager {}
    }

    pub fn set_active_count(&mut self, count: usize) -> Result<(), ()> {
        info!("Active guild count: {}", count);
        Ok(())
    }
}
