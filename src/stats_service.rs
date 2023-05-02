use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{models::League, rest_client::{self}, models2::external::game_stats::GameStatsV2, db::Db};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GTeamStats {
    g: i32,
    sog: i32,
    pim: i32,
    fow: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GHomeAwayStats {
    home: GTeamStats,
    away: GTeamStats,
}


impl From<GameStatsV2> for GHomeAwayStats {
    fn from(v: GameStatsV2) -> Self {
        let stats = v.period_stats_breakdown.iter()
            .find(|e| e.period.value.to_str() == "Total")
            .map(|e| e.statistics.clone());
        
        let goals = &stats.as_ref().and_then(|e| e.iter().find(|e| e.caption == "G"));
        let sog = &stats.as_ref().and_then(|e| e.iter().find(|e| e.caption == "SOG"));
        let fow = &stats.as_ref().and_then(|e| e.iter().find(|e| e.caption == "FOWon"));
        let pim = &stats.as_ref().and_then(|e| e.iter().find(|e| e.caption == "PIM"));

        let home = GTeamStats { 
            g: goals.map(|e| e.homeTeamValue).unwrap_or_default(), 
            sog: sog.map(|e| e.homeTeamValue).unwrap_or_default(),
            pim: pim.map(|e| e.homeTeamValue).unwrap_or_default(),
            fow: fow.map(|e| e.homeTeamValue).unwrap_or_default(),
        };

        let away = GTeamStats { 
            g: goals.map(|e| e.awayTeamValue).unwrap_or_default(), 
            sog: sog.map(|e| e.awayTeamValue).unwrap_or_default(),
            pim: pim.map(|e| e.awayTeamValue).unwrap_or_default(),
            fow: fow.map(|e| e.awayTeamValue).unwrap_or_default(),
        };
        GHomeAwayStats { home, away }
    }
}
pub struct StatsService;

impl StatsService {
    pub async fn update(league: &League, game_uuid: &str, throttle_s: Option<Duration>) -> Option<GHomeAwayStats> {
        let url = rest_client::get_stats_url(league, game_uuid);
        let rsp: Option<GameStatsV2> = rest_client::throttle_call(&url, throttle_s).await;
        rsp.map(|e| e.into())
    }

    pub fn is_stale(league: &League, game_uuid: &str) -> bool {
        let url = rest_client::get_stats_url(league, game_uuid);
        let db = Db::<String, GameStatsV2>::new("rest");
        db.is_stale(&url, None)
    }
}
