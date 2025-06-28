use std::sync::OnceLock;
use tokio::sync::{
    mpsc::{Receiver, Sender, channel},
    oneshot,
};

struct SetKeyValueMessage {
    key: String,
    value: Vec<u8>,
    response: oneshot::Sender<()>,
}

struct GetKeyValueMessage {
    key: String,
    response: oneshot::Sender<Option<Vec<u8>>>,
}

struct DeleteKeyValueMessage {
    key: String,
    response: oneshot::Sender<()>,
}

enum KeyValueMessage {
    Get(GetKeyValueMessage),
    Set(SetKeyValueMessage),
    Delete(DeleteKeyValueMessage),
}

enum RoutingMessage {
    KeyValue(KeyValueMessage),
}

async fn key_value_actor(mut receiver: Receiver<KeyValueMessage>) {
    let mut map = std::collections::HashMap::new();
    while let Some(message) = receiver.recv().await {
        match message {
            KeyValueMessage::Get(GetKeyValueMessage { key, response }) => {
                response.send(map.get(&key).cloned());
            }
            KeyValueMessage::Delete(DeleteKeyValueMessage { key, response }) => {
                map.remove(&key);
                response.send(());
            }
            KeyValueMessage::Set(SetKeyValueMessage {
                key,
                value,
                response,
            }) => {
                map.insert(key, value);
                response.send(());
            }
        }
    }
}

async fn router(mut receiver: Receiver<RoutingMessage>) {
    let (key_value_sender, key_value_receiver) = channel(32);
    tokio::spawn(key_value_actor(key_value_receiver));

    while let Some(message) = receiver.recv().await {
        match message {
            RoutingMessage::KeyValue(message) => {
                key_value_sender.send(message).await;
            }
        }
    }
}

static ROUTER_SENDER: OnceLock<Sender<RoutingMessage>> = OnceLock::new();

pub async fn set(key: String, value: Vec<u8>) -> Result<(), std::io::Error> {
    let (tx, rx) = oneshot::channel();
    ROUTER_SENDER
        .get()
        .unwrap()
        .send(RoutingMessage::KeyValue(KeyValueMessage::Set(
            SetKeyValueMessage {
                key,
                value,
                response: tx,
            },
        )))
        .await
        .unwrap();
    rx.await.unwrap();
    Ok(())
}

pub async fn get(key: String) -> Result<Option<Vec<u8>>, std::io::Error> {
    let (tx, rx) = oneshot::channel();
    ROUTER_SENDER
        .get()
        .unwrap()
        .send(RoutingMessage::KeyValue(KeyValueMessage::Get(
            GetKeyValueMessage { key, response: tx },
        )))
        .await
        .unwrap();
    Ok(rx.await.unwrap())
}

pub async fn delete(key: String) -> Result<(), std::io::Error> {
    let (tx, rx) = oneshot::channel();
    ROUTER_SENDER
        .get()
        .unwrap()
        .send(RoutingMessage::KeyValue(KeyValueMessage::Delete(
            DeleteKeyValueMessage { key, response: tx },
        )))
        .await
        .unwrap();
    rx.await.unwrap();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let (sender, receiver) = channel(32);
    ROUTER_SENDER.set(sender).unwrap();
    tokio::spawn(router(receiver));

    set("hello".to_owned(), b"world".to_vec()).await?;
    let value = get("hello".to_owned()).await?;
    println!("value: {:?}", String::from_utf8(value.unwrap_or_default()));
    delete("hello".to_owned()).await?;
    let value = get("hello".to_owned()).await?;
    println!("value after delete: {:?}", value);

    Ok(())
}
