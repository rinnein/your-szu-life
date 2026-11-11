pub mod commands;
pub mod electricity;
pub mod model;
mod scripts;
mod service;
mod state;
mod urls;

pub use electricity::{ElectricityCampus, ElectricityQueryInput, ElectricityQueryView};
pub use model::{ElectricityLocationSettings, ElectricityStatusView};
pub use service::{cancel, on_page_load, window_closed};
pub use state::CampusPaymentsStore;

const PORTAL_URL: &str = "https://self.szu.edu.cn/self/nav_operators";

#[cfg(test)]
mod tests;
