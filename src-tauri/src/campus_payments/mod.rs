pub(crate) mod commands;
pub(crate) mod electricity;
pub(crate) mod mcp;
pub(crate) mod model;
mod scripts;
mod service;
mod state;
mod urls;

pub(crate) use electricity::{ElectricityCampus, ElectricityQueryInput, ElectricityQueryView};
pub(crate) use model::{ElectricityLocationSettings, ElectricityStatusView};
pub(crate) use service::{cancel, on_page_load, window_closed};
pub(crate) use state::CampusPaymentsStore;

const PORTAL_URL: &str = "https://self.szu.edu.cn/self/nav_operators";

#[cfg(test)]
mod tests;
