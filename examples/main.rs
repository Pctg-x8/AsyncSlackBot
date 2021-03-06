extern crate asyncslackbot;

use asyncslackbot::*;

pub struct TestSlackbot { mention_header: String }
impl TestSlackbot {
    pub fn new() -> Self {
        TestSlackbot { mention_header: "".to_owned() }
    }
}
impl SlackBotLogic for TestSlackbot {
    fn launch(_: &AsyncSlackApiSender, botinfo: &ConnectionAccountInfo, _: &TeamInfo) -> Self {
        println!("Connecting as {}", botinfo.name);
        TestSlackbot {
            mention_header: format!("<@{}>", botinfo.id)
        }
    }
    fn on_message(&mut self, api_sender: &AsyncSlackApiSender, e: MessageEvent) {
        let text_trimmed = e.text.trim();
        let stripped = text_trimmed.split(&self.mention_header);
        if let Some(s) = stripped.skip(1).next().map(|s| s.trim()) {
            println!(">> Incoming Command: {:?} from {} at {} in {}", s, e.user, e.ts, e.channel);
            if let Some(resp) = self.recognize_command(s) {
                println!("ResponseEmu: {:?}", resp);
                chat::post_message(api_sender, chat::PostMessageParams {
                    channel: e.channel, text: &resp,
                    .. Default::default()
                });
            }
            else {
                reactions::add(api_sender, reactions::AddRequestParams {
                    name: "no_entry_sign", timestamp: e.ts, channel: e.channel
                });
            }
        }
        else {
            println!("Not a mention to a bot");
        }
    }
}
impl TestSlackbot {
    pub fn recognize_command(&self, c: &str) -> Option<String> {
        if c == "share-play" {
            return Some("share play".to_owned());
        }
        return None;
    }
}

fn main() { launch_rtm::<TestSlackbot>(env!("SLACK_API_TOKEN")); }
