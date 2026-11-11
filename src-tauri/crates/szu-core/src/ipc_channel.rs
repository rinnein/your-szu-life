use std::sync::Mutex;
use tauri::ipc::Channel;

pub struct SubscriberPool<T: Clone + serde::Serialize + Send + 'static> {
    channels: Mutex<Vec<Channel<T>>>,
}

impl<T: Clone + serde::Serialize + Send + 'static> Default for SubscriberPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + serde::Serialize + Send + 'static> SubscriberPool<T> {
    pub fn new() -> Self {
        Self {
            channels: Mutex::new(Vec::new()),
        }
    }

    pub fn subscribe(&self, channel: Channel<T>) {
        let mut list = self.channels.lock().unwrap();
        list.push(channel);
    }

    pub fn is_empty(&self) -> bool {
        self.channels.lock().unwrap().is_empty()
    }

    pub fn broadcast(&self, event: &T) {
        let mut list = self.channels.lock().unwrap();
        list.retain(|channel| channel.send(event.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn collecting_channel<
        T: Clone + serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    >() -> (Channel<T>, Arc<Mutex<Vec<T>>>) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let target = sink.clone();
        let channel = Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                if let Ok(value) = serde_json::from_str::<T>(&json) {
                    target.lock().unwrap().push(value);
                }
            }
            Ok(())
        });
        (channel, sink)
    }

    #[test]
    fn broadcasts_to_every_subscriber() {
        let pool: SubscriberPool<u32> = SubscriberPool::new();
        assert!(pool.is_empty());

        let (first, first_sink) = collecting_channel::<u32>();
        let (second, second_sink) = collecting_channel::<u32>();
        pool.subscribe(first);
        pool.subscribe(second);
        assert!(!pool.is_empty());

        pool.broadcast(&7);
        assert_eq!(*first_sink.lock().unwrap(), vec![7]);
        assert_eq!(*second_sink.lock().unwrap(), vec![7]);
    }
}
