#[derive(Debug)]
pub enum Event {
    Msg(Message),
}

#[derive(Debug)]
pub struct Message {
    username: String,
    msg: String,
}