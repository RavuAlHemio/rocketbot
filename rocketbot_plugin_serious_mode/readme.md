When the `srs` command is triggered, the following structure is added into the behavior flags:

    {
        "srs": {
            "GENERAL": 1632823700
        }
    }

where `"GENERAL"` is the channel ID and `1632823700` is the Unix timestamp when Serious Mode ends.

If course, if an object already exists behind `"srs"`, it is updated (and not entirely replaced).

To integrate your plugin, query `behavior_flags["srs"][channel_id]` and compare the timestamp to the current timestamp, e.g.:

    let behavior_flags = serde_json::Value::Object(interface.obtain_behavior_flags().await);
    if let Some(serious_mode_until) = behavior_flags["srs"][&channel_message.channel.id].as_i64() {
        if serious_mode_until > Local::now().timestamp() {
            return;
        }
    }
