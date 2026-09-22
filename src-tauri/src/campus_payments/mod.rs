pub(crate) mod commands;
pub(crate) mod electricity;
pub(crate) mod model;
mod scripts;
mod service;
mod state;
mod urls;

pub(crate) use electricity::{ElectricityCampus, ElectricityQueryInput, ElectricityQueryView};
pub(crate) use model::{ElectricityLocationSettings, ElectricityStatusView};
pub(crate) use service::{cancel, on_page_load};
pub(crate) use state::CampusPaymentsStore;

const PORTAL_URL: &str = "https://self.szu.edu.cn/self/nav_operators";
// The network-fee command is owned by the nested network tab, not the
// parent campus-services shell route. Keep the return URL bound to the exact
// page that initiated the official portal flow.
const PAYMENTS_PATH: &str = "/campus-payments/network";

#[cfg(test)]
mod tests;
