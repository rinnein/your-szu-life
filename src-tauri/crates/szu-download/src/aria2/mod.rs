pub mod binary;
pub mod client;
pub mod downloader;
pub mod process;

#[cfg(test)]
mod tests;

pub use downloader::{
    download_with_aria2, download_with_aria2_controlled, DownloadControl, DownloadJobOptions,
    DownloadPhase, DownloadTarget, PublishTarget,
};
