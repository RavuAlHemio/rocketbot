#[macro_export]
macro_rules! send_channel_message {
    ($interface:expr, $channel_name:expr, $message:expr $(,)?) => {
        {
            log::debug!("sending message {:?} to channel {:?} at {}:{}:{}", $message, $channel_name, file!(), line!(), column!());
            $interface.send_channel_message($channel_name, $message)
        }
    };
}

#[macro_export]
macro_rules! send_private_message {
    ($interface:expr, $conversation_id:expr, $message:expr $(,)?) => {
        {
            log::debug!("sending message {:?} to private conversation {:?} at {}:{}:{}", $message, $conversation_id, file!(), line!(), column!());
            $interface.send_private_message($conversation_id, $message)
        }
    };
}

#[macro_export]
macro_rules! send_private_message_to_user {
    ($interface:expr, $username:expr, $message:expr $(,)?) => {
        {
            log::debug!("sending private message {:?} to user {:?} at {}:{}:{}", $message, $username, file!(), line!(), column!());
            $interface.send_private_message_to_user($username, $message)
        }
    };
}

#[macro_export]
macro_rules! send_channel_message_advanced {
    ($interface:expr, $channel_name:expr, $message:expr $(,)?) => {
        {
            log::debug!("sending advanced message {:?} to channel {:?} at {}:{}:{}", $message, $channel_name, file!(), line!(), column!());
            $interface.send_channel_message_advanced($channel_name, $message)
        }
    };
}

#[macro_export]
macro_rules! send_private_message_advanced {
    ($interface:expr, $conversation_id:expr, $message:expr $(,)?) => {
        {
            log::debug!("sending advanced message {:?} to private conversation {:?} at {}:{}:{}", $message, $conversation_id, file!(), line!(), column!());
            $interface.send_private_message_advanced($conversation_id, $message)
        }
    };
}

#[macro_export]
macro_rules! send_private_message_to_user_advanced {
    ($interface:expr, $username:expr, $message:expr $(,)?) => {
        {
            log::debug!("sending advanced private message {:?} to user {:?} at {}:{}:{}", $message, $username, file!(), line!(), column!());
            $interface.send_private_message_to_user_advanced($username, $message)
        }
    };
}
