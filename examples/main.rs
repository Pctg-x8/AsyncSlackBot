extern crate asyncslackbot;

use asyncslackbot::*;

pub struct TestSlackbot { apisender: AsyncSlackApiSender, mention_header: String }
impl SlackBotLogic for TestSlackbot
{
    fn launch(apisender: AsyncSlackApiSender, botinfo: &ConnectionAccountInfo, _: &TeamInfo) -> Self
    {
        println!("Connecting as {}", botinfo.name);

        TestSlackbot
        {
            apisender,
            mention_header: format!("<@{}>", botinfo.id)
        }
    }
    fn on_message(&mut self, e: MessageEvent)
    {
        let text_trimmed = e.text.trim();
        let stripped = text_trimmed.split(&self.mention_header);
        if let Some(s) = stripped.skip(1).next().map(|s| s.trim())
        {
            println!(">> Incoming Command: {:?} from {} at {} in {}", s, e.user, e.ts, e.channel);

            if let Some(resp) = self.recognize_command(s)
            {
                println!("ResponseEmu: {:?}", resp);

                async_std::task::spawn(reqwest::Client::new().execute(chat::PostMessageParams
                {
                    channel: e.channel, text: &resp,
                    .. Default::default()
                }.to_post_request(&self.apisender.tok)));
            }
            else
            {
                async_std::task::spawn(reqwest::Client::new().execute(reactions::AddRequestParams
                {
                    name: "no_entry_sign", timestamp: e.ts, channel: e.channel
                }.to_post_request(&self.apisender.tok)));
            }
        }
        else
        {
            println!("Not a mention to a bot");
        }
    }
}
impl TestSlackbot
{
    pub fn recognize_command(&self, c: &str) -> Option<String>
    {
        if c == "share-play"
        {
            return Some("share play".to_owned());
        }
        return None;
    }
}

#[async_std::main]
async fn main()
{
    launch_rtm::<TestSlackbot>(std::env::var("SLACK_API_TOKEN").expect("missing $Env:SLACK_API_TOKEN")).await;
}
