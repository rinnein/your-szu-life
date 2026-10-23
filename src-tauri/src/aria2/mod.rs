pub(crate) mod binary;
pub(crate) mod client;
pub(crate) mod downloader;
pub(crate) mod process;

#[cfg(test)]
mod tests;

pub(crate) use downloader::{download_with_aria2, DownloadJobOptions};
