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
    pub user: &'s str, pub text: Cow<'s, str>, pub ts: &'s str, pub channel: &'s str
}

#[derive(Deserialize, Debug)]
pub struct GenericResult { ok: bool, error: Option<String> }

pub mod rtm {
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

    pub fn connect(token: &str) -> r::Result<ConnectResponseResult> {
        r::get(&format!("https://slack.com/api/rtm.connect?token={}", token))?.json()
    }
}
pub mod reactions {
    #[derive(Serialize, Debug)]
    pub struct AddRequestParams<'s> {
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
pub struct SlackWebApiCall {
    endpoint: &'static str, paramdata: String
}
pub trait SlackWebAPI: serde::Serialize {
    const EP: &'static str;

    fn to_apicall(&self) -> SlackWebApiCall {
        SlackWebApiCall { endpoint: Self::EP, paramdata: jsonify(self).unwrap() }
    }
}
#[derive(Clone)]
pub struct AsyncSlackApiSender(mpsc::Sender<SlackWebApiCall>);
impl AsyncSlackApiSender {
    pub fn send<P: SlackWebAPI + ?Sized>(&self, params: &P) {
        self.0.send(params.to_apicall()).expect("Failed to send SlackWebAPICall");
    }
}

use std::thread::{spawn, JoinHandle};
use std::sync::mpsc;
use reqwest::header::{Authorization, Bearer};
pub struct AsyncSlackWebApis {
    _th: JoinHandle<()>, sender: AsyncSlackApiSender
}
impl AsyncSlackWebApis {
    pub fn run(tok: String) -> Self {
        let (s, r) = mpsc::channel();
        let _th = spawn(move || {
            let c = reqwest::Client::new();
            loop {
                match r.recv() {
                    Ok(SlackWebApiCall { endpoint, paramdata }) => {
                        trace!("{}: {:?} {:?}", "Posting".bright_white().bold(), endpoint, paramdata);
                        let mut headers = r::header::Headers::new();
                        headers.set(Authorization(Bearer { token: tok.clone() }));
                        headers.set(r::header::ContentType::json());
                        match c.post(endpoint).headers(headers).body(paramdata).send() {
                            Ok(mut req) => {
                                let e = req.json::<GenericResult>().expect("Converting MethodResult");
                                if !e.ok { eprintln!("Err: Invalid Request? {:?}", e.error.unwrap()); }
                            },
                            Err(e) => eprintln!("Err in Requesting: {:?}", e)
                        }
                    },
                    Err(e) => Err(e).unwrap()
                }
            }
        });
        return AsyncSlackWebApis { _th, sender: AsyncSlackApiSender(s) }
    }
    fn sender(&self) -> &AsyncSlackApiSender { &self.sender }
}

pub struct SlackRtmHandler<Logic: SlackBotLogic> { _ws_outgoing: ws::Sender, logic: Logic, apihandler: AsyncSlackWebApis }
impl<Logic: SlackBotLogic> ws::Handler for SlackRtmHandler<Logic> {
    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        println!("Incoming Message from SlackRtm: {:?}", msg);
        match msg {
            ws::Message::Text(t) => {
                match serde_json::from_str::<Event>(&t) {
                    Ok(Event::Hello) => println!("Hello!"),
                    Ok(Event::Message { data }) =>
                        self.logic.on_message(self.apihandler.sender(), data),
                    _ => println!("Unknown Event")
                }
            },
            _ => println!("Unsupported WebSocket Message")
        }
        return Ok(());
    }
}

pub fn launch_rtm<L: SlackBotLogic>(api_token: &str) {
    let con = rtm::connect(api_token).unwrap();
    match con {
        rtm::ConnectResponseResult::Ok { url, team, self_, .. } => {
            ws::connect(url, move |sender| {
                let apihandler = AsyncSlackWebApis::run(api_token.to_owned());
                let logic = L::launch(apihandler.sender(), &self_, &team);
                SlackRtmHandler { _ws_outgoing: sender, logic, apihandler }
            }).unwrap();
        },
        rtm::ConnectResponseResult::Err { error, .. } => panic!("Error connecting SlackRTM: {}", error)
    }
}

#[allow(unused_variables)]
pub trait SlackBotLogic {
    fn launch(api: &AsyncSlackApiSender, botinfo: &ConnectionAccountInfo, teaminfo: &TeamInfo) -> Self;
    fn on_message(&mut self, api_sender: &AsyncSlackApiSender, event: MessageEvent) {}
}
