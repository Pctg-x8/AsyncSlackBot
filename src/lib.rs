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

    pub fn connect(token: &str) -> r::Result<ConnectResponse> {
        r::get(&format!("https://slack.com/api/rtm.connect?token={}", token))?.json()
    }
}
pub mod reactions {
    use std::sync::mpsc;

    #[derive(Serialize, Debug)]
    pub struct AddRequestParams<'s> {
        pub name: &'s str,
        pub channel: &'s str, pub timestamp: &'s str
    }

    pub fn add(api: &mpsc::Sender<super::SlackWebApi>, param: AddRequestParams) {
        api.send(super::SlackWebApi {
            endpoint: "https://slack.com/api/reactions.add",
            paramdata: super::jsonify(&param).unwrap()
        }).unwrap();
    }
}
pub mod chat {
    use std::sync::mpsc;

    #[derive(Serialize, Debug)]
    pub struct PostMessageParams<'s> {
        pub channel: &'s str, pub text: &'s str,
        pub as_user: Option<bool>, pub icon_emoji: Option<&'s str>, pub icon_url: Option<&'s str>, pub username: Option<&'s str>
    }
    impl<'s> Default for PostMessageParams<'s> {
        fn default() -> Self {
            PostMessageParams {
                channel: "", text: "",
                as_user: None, icon_emoji: None, icon_url: None, username: None
            }
        }
    }

    pub fn post_message(api: &mpsc::Sender<super::SlackWebApi>, param: PostMessageParams) {
        api.send(super::SlackWebApi {
            endpoint: "https://slack.com/api/chat.postMessage",
            paramdata: super::jsonify(&param).unwrap()
        }).unwrap();
    }
}

#[derive(Debug)]
pub struct SlackWebApi {
    endpoint: &'static str, paramdata: String
}

pub type AsyncSlackApiSender = mpsc::Sender<SlackWebApi>;

use std::thread::{spawn, JoinHandle};
use std::sync::mpsc;
use reqwest::header::{Authorization, Bearer};
pub struct AsyncSlackWebApis {
    th: JoinHandle<()>, sender: mpsc::Sender<SlackWebApi>
}
impl AsyncSlackWebApis {
    pub fn run(tok: String) -> Self {
        let (s, r) = mpsc::channel();
        let th = spawn(move || {
            let c = reqwest::Client::new();
            loop {
                match r.recv() {
                    Ok(SlackWebApi { endpoint, paramdata }) => {
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
        return AsyncSlackWebApis { th, sender: s }
    }
    fn sender(&self) -> &mpsc::Sender<SlackWebApi> { &self.sender }
}

pub struct SlackRtmHandler<Logic: SlackBotLogic> { ws_outgoing: ws::Sender, logic: Logic, apihandler: AsyncSlackWebApis }
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
    let rtm::ConnectResponse { url, team, self_, .. } = rtm::connect(api_token).unwrap();
    ws::connect(url, move |sender| {
        let apihandler = AsyncSlackWebApis::run(api_token.to_owned());
        let logic = L::launch(apihandler.sender(), &self_, &team);
        SlackRtmHandler { ws_outgoing: sender, logic, apihandler }
    }).unwrap();
}

#[allow(unused_variables)]
pub trait SlackBotLogic {
    fn launch(api: &AsyncSlackApiSender, botinfo: &ConnectionAccountInfo, teaminfo: &TeamInfo) -> Self;
    fn on_message(&mut self, api_sender: &AsyncSlackApiSender, event: MessageEvent) {}
}
