use std::{time::Duration, str::FromStr, fmt::{Display, write}};

use serde::{Deserialize, Serialize};
use tracing::log;

use crate::{db::Db, rest_client::{get_events, self}, models2::external::{event::{PlayByPlayType, Penalty, Shot, Goal}, self}, game_report_service::{GameStatus, ApiGameReport}, models::ParseStringError};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Player {
    first_name: String,
    family_name: String,
    jersey: String,
}
impl FromStr for Player {
    type Err = ParseStringError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 1 Johan Johansson Olsson => Player
        let parts: Vec<&str> = s.split(' ').collect();
        let jersey = parts.first().cloned().unwrap_or_default().to_string(); 
        let first_name = parts.get(1).cloned().unwrap_or_default().to_string();
        let family_name = s.replace(format!("{jersey} {first_name}").as_str(), "");
        Ok(Player { jersey, first_name, family_name })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Location {
    x: f32,
    y: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct GoalInfo {
    pub team: String,
    pub player: Option<Player>,
    pub team_advantage: String,
    pub assist: Option<String>,
    pub home_team_result: i16,
    pub away_team_result: i16,
    pub location: Location,
}

impl GoalInfo {
    pub fn new(a: &Goal) -> GoalInfo {
        GoalInfo { 
            team: a.team.clone(),
            player: a.extra.scorerLong.parse().ok(),
            team_advantage: a.extra.teamAdvantage.clone(),
            assist: Some(a.extra.assist.clone()),
            home_team_result: a.extra.homeForward.to_num(),
            away_team_result: a.extra.homeAgainst.to_num(),
            location: Location { x: a.location.x, y: a.location.y }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct PenaltyInfo {
    pub team: String,
    pub player: Option<Player>,
    pub reason: Option<String>,
    pub penalty: Option<String>,
}
impl PenaltyInfo {
    pub fn new(description: &str, p: &Penalty) -> PenaltyInfo {
        let (player_info, penalty_info) = description.split_once("utvisas ")
            .map(|e| (Some(e.0), Some(e.1)))
            .unwrap_or_else(|| (None, None));
        let (penalty, reason) = penalty_info.unwrap_or_default().split_once(',')
            .map(|e| (Some(e.0.to_string()), Some(e.1.to_string())))
            .unwrap_or_else(|| (None, None));
        let player = player_info.unwrap_or_default().parse::<Player>().ok();
        PenaltyInfo { team: p.team.clone(), player, reason, penalty }
    }
}


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ShotInfo {
    pub team: String,
    pub location: Location,
}
impl ShotInfo {
    pub fn new(info: &Shot) -> ShotInfo {
        ShotInfo { team: info.team.clone(), location: Location { x: info.location.x, y: info.location.y } }
    }
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GameEndInfo {
    pub winner: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", content = "info")]
pub enum ApiEventType {
    Goal(GoalInfo),
    PeriodEnd,
    PeriodStart,
    GameEnd(GameEndInfo),
    GameStart,
    Penalty(PenaltyInfo),
    Shot(ShotInfo),
    Timeout,
    General,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiGameEvent {
    pub game_uuid: String,
    pub event_id: String,
    pub revision: u16,
    pub status: GameStatus,
    pub gametime: String,
    pub description: String,
    #[serde(flatten)]
    pub info: ApiEventType,
}

impl ApiGameEvent {
    pub fn should_publish(&self) -> bool {
        matches!(self.info, ApiEventType::Goal(_) | ApiEventType::GameStart | ApiEventType::GameEnd(_))
    }
}
impl Display for ApiGameEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {} :: {:?} • {}", self.info, self.description, self.status, self.gametime)
    }
}

impl external::event::PlayByPlay {
    fn to_type(&self) -> ApiEventType {
        match &self.class {
            PlayByPlayType::General(_) => ApiEventType::General,
            PlayByPlayType::Livefeed(_) => ApiEventType::General,
            PlayByPlayType::GoolkeeperEvent(_) => ApiEventType::General,

            PlayByPlayType::Goal(a) => ApiEventType::Goal(GoalInfo::new(a)),

            PlayByPlayType::Shot(a) =>          ApiEventType::Shot(ShotInfo::new(a)),
            PlayByPlayType::ShotBlocked(a) =>   ApiEventType::Shot(ShotInfo::new(a)),
            PlayByPlayType::ShotWide(a) =>      ApiEventType::Shot(ShotInfo::new(a)),
            PlayByPlayType::ShotIron(a) =>      ApiEventType::Shot(ShotInfo::new(a)),
            PlayByPlayType::PenaltyShot(a) =>   ApiEventType::Shot(ShotInfo::new(a)),
            PlayByPlayType::ShootoutPenaltyShot(a) => ApiEventType::Shot(ShotInfo::new(a)),

            PlayByPlayType::Penalty(a) => ApiEventType::Penalty(PenaltyInfo::new(&self.description, a)),

            PlayByPlayType::Timeout(_) => ApiEventType::Timeout,

            PlayByPlayType::Period(a) => match a.extra.gameStatus.as_str() {
                "Playing" => ApiEventType::PeriodStart,
                _ => ApiEventType::PeriodEnd,
            },
        }
    }
}

impl external::event::PlayByPlay {
    pub fn into_mapped_event(self, game_uuid: &str) -> ApiGameEvent {
        let info: ApiEventType = self.to_type();
        ApiGameEvent {
            game_uuid: game_uuid.to_string(),
            event_id: format!("{}", self.eventId),
            revision: self.revision,
            status: self.period.to_num().into(),
            gametime: self.gametime.clone(),
            description: self.description,
            info,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayByPlay {
    pub game_uuid: String,
    pub event_id: i32,
    pub revision: u16,
    pub period: i16,
    pub gametime: String,
    pub description: String,
    pub class: PlayByPlayType,
}

impl external::event::PlayByPlay {
    pub fn into_mapped(self, game_uuid: &str) -> PlayByPlay {
        PlayByPlay {
            game_uuid: game_uuid.to_string(),
            event_id: self.eventId,
            revision: self.revision,
            period: self.period.to_num(),
            gametime: self.gametime.clone(),
            description: self.description.clone(),
            class: self.class,
        }
    }
}

pub struct EventService;
impl EventService {
 
    pub async fn update(game_uuid: &str, throttle_s: Option<Duration>) -> Option<Vec<ApiGameEvent>> {
        let db_raw: Db<String, Vec<external::event::PlayByPlay>> = Db::new("v2_events_raw");
        // let db: Db<String, Vec<ApiGameEvent>> = Db::new("v2_events_2");

        
        let raw_events = if !db_raw.is_stale(&game_uuid.to_string(), throttle_s) {
            db_raw.read(&game_uuid.to_string()).unwrap_or_default()
        } else {
            rest_client::get_events(game_uuid).await.unwrap_or_default()
        };
        db_raw.write(&game_uuid.to_string(), &raw_events);

        Some(raw_events.into_iter().map(|e| e.into_mapped_event(game_uuid)).collect())
    }

    pub fn store_raw(game_uuid: &str, event: &external::event::PlayByPlay) -> bool {
        let db = Db::<String, Vec<external::event::PlayByPlay>>::new("v2_events_raw");
        let mut events = db.read(&game_uuid.to_string()).unwrap_or_default();
        let new_event;
        if let Some(pos) = events.iter().position(|e| e.eventId == event.eventId) {
            events[pos] = event.clone();
            new_event = false;
        } else {
            events.push(event.clone());
            new_event = true;
        }
        db.write(&game_uuid.to_string(), &events);
        new_event
    }

    pub fn store(game_uuid: &str, event: &ApiGameEvent) -> bool {
        let db = Db::<String, Vec<ApiGameEvent>>::new("v2_events_2");
        let mut events: Vec<ApiGameEvent> = db.read(&game_uuid.to_string()).unwrap_or_default();
        let new_event;
        if let Some(pos) = events.iter().position(|e| e.event_id == event.event_id) {
            events[pos] = event.clone();
            new_event = false;
        } else {
            events.push(event.clone());
            new_event = true;
        }
        db.write(&game_uuid.to_string(), &events);
        new_event
    }

    pub fn read(game_uuid: &str) -> Vec<PlayByPlay> {
        Db::new("v2_events_raw").read(&game_uuid).unwrap_or_default()
    }
}
