extern crate ws;
extern crate reqwest;

extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;

extern crate colored;
#[macro_use] extern crate log;

use reqwest as r;
use colored::*;
use serde_json::to_string as jsonify;
use std::borrow::Cow;

#[derive(Deserialize, Debug)]
pub struct TeamInfo { pub id: String, pub name: String, pub domain: String }
#[derive(Deserialize, Debug)]
pub struct ConnectionAccountInfo { pub id: String, pub name: String }

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all="camelCase")]
pub enum Event<'s> {
    Hello, Message { #[serde(flatten, borrow)] data: MessageEvent<'s> }
}
#[derive(Deserialize, Debug)]
pub struct MessageEvent<'s> {
    pub user: &'s str, pub text: Cow<'s, str>, pub ts: &'s str, pub channel: &'s str, pub subtype: Option<&'s str>
}

#[derive(Deserialize, Debug)]
pub struct GenericResult { ok: bool, error: Option<String> }

pub mod rtm
{
    use reqwest as r;
    #[derive(Deserialize, Debug)]
    pub struct ConnectResponse {
        ok: bool, pub url: String,
        pub team: super::TeamInfo,
        #[serde(rename = "self")] pub self_: super::ConnectionAccountInfo
    }
    #[derive(Deserialize, Debug)] #[serde(untagged)] pub enum ConnectResponseResult {
        Ok { ok: bool, url: String, team: super::TeamInfo, #[serde(rename = "self")] self_: super::ConnectionAccountInfo },
        Err { ok: bool, error: String }
    }

    pub async fn connect(token: &str) -> r::Result<ConnectResponseResult>
    {
        r::get(&format!("https://slack.com/api/rtm.connect?token={}", token)).await?.json().await
    }
}
pub mod reactions
{
    #[derive(Serialize, Debug)]
    pub struct AddRequestParams<'s>
    {
        pub name: &'s str,
        pub channel: &'s str, pub timestamp: &'s str
    }
    impl<'s> super::SlackWebAPI for AddRequestParams<'s> {
        const EP: &'static str = "https://slack.com/api/reactions.add";
    }
}
pub mod chat {
    use std::borrow::Cow;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Attachment<'s> {
        pub color: Option<Cow<'s, str>>, pub text: Option<Cow<'s, str>>
    }
    impl<'s> Default for Attachment<'s> {
        fn default() -> Self {
            Attachment { color: None, text: None }
        }
    }

    #[derive(Serialize, Debug)]
    pub struct PostMessageParams<'s> {
        pub channel: &'s str, pub text: &'s str,
        pub as_user: Option<bool>, pub icon_emoji: Option<&'s str>, pub icon_url: Option<&'s str>, pub username: Option<&'s str>,
        pub attachments: Vec<Attachment<'s>>
    }
    impl<'s> Default for PostMessageParams<'s> {
        fn default() -> Self {
            PostMessageParams {
                channel: "", text: "",
                as_user: None, icon_emoji: None, icon_url: None, username: None,
                attachments: Vec::new()
            }
        }
    }
    impl<'s> super::SlackWebAPI for PostMessageParams<'s> {
        const EP: &'static str = "https://slack.com/api/chat.postMessage";
    }
}
pub mod conversations {
    #[derive(Serialize, Debug)]
    pub struct HistoryParams<'s> {
        pub channel: &'s str, limit: usize, inclusive: bool,
        pub latest: Option<&'s str>, pub oldest: Option<&'s str>, pub unreads: bool
    }
    impl<'s> Default for HistoryParams<'s> {
        fn default() -> Self {
            HistoryParams {
                channel: "", limit: 100, inclusive: false, latest: None, oldest: None, unreads: false
            }
        }
    }
    impl<'s> super::SlackWebAPI for HistoryParams<'s> {
        const EP: &'static str = "https://slack.com/api/conersations.history";
    }
}

#[derive(Debug)]
pub struct SlackWebApiCall
{
    endpoint: &'static str, paramdata: String
}
pub trait SlackWebAPI: serde::Serialize
{
    const EP: &'static str;

    fn to_apicall(&self) -> SlackWebApiCall
    {
        SlackWebApiCall { endpoint: Self::EP, paramdata: jsonify(self).unwrap() }
    }
    fn to_post_request(&self, tok: &str) -> r::Request
    {
        let mut req = r::Request::new(r::Method::POST, r::Url::parse(Self::EP).expect("invalid url"));
        req.headers_mut()
            .insert(r::header::CONTENT_TYPE, r::header::HeaderValue::from_static("application/json"));
        req.headers_mut()
            .insert(r::header::AUTHORIZATION, r::header::HeaderValue::from_str(&format!("Bearer {}", tok)).unwrap());
        *req.body_mut() = Some(r::Body::from(jsonify(self).unwrap()));

        req
    }
}
pub struct AsyncSlackApiSender {
    pub tok: String
}
impl AsyncSlackApiSender {
    pub fn new(tok: String) -> Self {
        AsyncSlackApiSender { tok }
    }
    
    pub async fn send(&self, callinfo: SlackWebApiCall)
    {
        trace!("{}: {:?} {:?}", "Posting".bright_white().bold(), callinfo.endpoint, callinfo.paramdata);
        let resp = r::Client::new().post(callinfo.endpoint)
            .bearer_auth(self.tok.clone())
            .header(r::header::CONTENT_TYPE, "application/json")
            .body(callinfo.paramdata)
            .send().await;
        
        match resp
        {
            Ok(req) =>
            {
                let e = req.json::<GenericResult>().await.expect("Converting MethodResult");
                if !e.ok { eprintln!("Err: Invalid Request? {:?}", e.error.unwrap()); }
            },
            Err(e) => eprintln!("Err in Requesting: {:?}", e)
        }
    }
}

pub struct SlackRtmHandler<Logic: SlackBotLogic>
{
    _ws_outgoing: ws::Sender, logic: Logic
}
impl<Logic: SlackBotLogic> ws::Handler for SlackRtmHandler<Logic>
{
    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()>
    {
        debug!("Incoming Message from SlackRtm: {:?}", msg);

        match msg
        {
            ws::Message::Text(t) =>
            {
                match serde_json::from_str::<Event>(&t)
                {
                    Ok(Event::Hello) => println!("Hello!"),
                    Ok(Event::Message { data }) => self.logic.on_message(data),
                    _ => println!("Unknown Event")
                }
            },
            _ => println!("Unsupported WebSocket Message")
        }

        Ok(())
    }
}

pub async fn launch_rtm<L: SlackBotLogic>(api_token: String) {
    match rtm::connect(&api_token).await.unwrap() {
        rtm::ConnectResponseResult::Ok { url, team, self_, .. } => {
            ws::connect(url, move |sender| SlackRtmHandler {
                _ws_outgoing: sender,
                logic: L::launch(AsyncSlackApiSender::new(api_token.clone()), &self_, &team)
            }).unwrap();
        },
        rtm::ConnectResponseResult::Err { error, .. } => panic!("Error connecting SlackRTM: {}", error)
    }
}

#[allow(unused_variables)]
pub trait SlackBotLogic
{
    fn launch(api: AsyncSlackApiSender, botinfo: &ConnectionAccountInfo, teaminfo: &TeamInfo) -> Self;

    fn on_message(&mut self, event: MessageEvent) {}
}
