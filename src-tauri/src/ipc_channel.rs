use std::sync::Mutex;
use tauri::ipc::Channel;

pub(crate) struct SubscriberPool<T: Clone + serde::Serialize + Send + 'static> {
    channels: Mutex<Vec<Channel<T>>>,
}

impl<T: Clone + serde::Serialize + Send + 'static> Default for SubscriberPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + serde::Serialize + Send + 'static> SubscriberPool<T> {
    pub(crate) fn new() -> Self {
        Self {
            channels: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn subscribe(&self, channel: Channel<T>) {
        let mut list = self.channels.lock().unwrap();
        list.push(channel);
    }

    pub(crate) fn broadcast(&self, event: &T) {
        let mut list = self.channels.lock().unwrap();
        list.retain(|channel| channel.send(event.clone()).is_ok());
    }
}
