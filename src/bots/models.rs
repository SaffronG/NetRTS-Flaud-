use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitType {
    Worker,
    Soldier,
    Archer,
    Scout,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingType {
    CommandCenter,
    Barracks,
    ResourceDepot,
    TechLab,
    GuardTower,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpgradeType {
    Weapons1,
    Weapons2,
    Armor1,
    Armor2,
    Mobility1,
    Mobility2,
    Harvesting1,
    Harvesting2,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandType {
    Move,
    Attack,
    Gather,
    Build,
    Produce,
    Research,
    Stop,
    Rally,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitActivity {
    Idle,
    Moving,
    Attacking,
    Gathering,
    Returning,
    Building,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchStatus {
    Waiting,
    Active,
    Completed,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchEndReason {
    Elimination,
    TimeLimit,
    Surrender,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiErrorDto {
    pub code: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PositionDto {
    pub x: i32,
    pub y: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScoreDto {
    pub destruction: i32,
    pub economy: i32,
    pub survival: i32,
    pub total: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MatchOutcomeDto {
    pub reason: MatchEndReason,
    pub ticks: i32,
    pub winner_id: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct RegisterPlayerRequest {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayerResponse {
    pub player_id: String,
    pub name: String,
    pub api_key: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct HouseBotDto {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MatchSettingsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ticks: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_interval_ms: Option<i32>,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateMatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub house_bots: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_players: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<MatchSettingsDto>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MatchPlayerDto {
    pub is_house_bot: bool,
    pub name: String,
    pub player_id: String,
    pub slot: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MatchSummaryDto {
    pub match_id: String,
    pub created_at: String,
    pub status: MatchStatus,
    pub map_height: i32,
    pub map_width: i32,
    pub max_players: i32,
    pub max_ticks: i32,
    pub seed: i32,
    pub tick: i32,
    pub tick_interval_ms: i32,
    pub outcome: Option<MatchOutcomeDto>,
    #[serde(default)]
    pub players: Vec<MatchPlayerDto>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RulesDto {
    pub economy: EconomyRulesDto,
    #[serde(default)]
    pub buildings: Vec<BuildingStatsDto>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub units: Vec<UnitStatsDto>,
    #[serde(default)]
    pub upgrades: Vec<UpgradeStatsDto>,
}

impl RulesDto {
    pub fn unit(&self, t: UnitType) -> Option<&UnitStatsDto> {
        self.units.iter().find(|u| u.kind == t)
    }
    pub fn building(&self, t: BuildingType) -> Option<&BuildingStatsDto> {
        self.buildings.iter().find(|b| b.kind == t)
    }
    pub fn upgrade(&self, t: UpgradeType) -> Option<&UpgradeStatsDto> {
        self.upgrades.iter().find(|u| u.kind == t)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EconomyRulesDto {
    pub gather_per_tick: i32,
    pub max_production_queue: i32,
    pub starting_resources: i32,
    pub starting_workers: i32,
    pub worker_carry_capacity: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BuildingStatsDto {
    #[serde(rename = "type")]
    pub kind: BuildingType,
    pub armor: i32,
    pub attack_cooldown: i32,
    pub build_work: i32,
    pub can_research: bool,
    pub cost: i32,
    pub damage: i32,
    pub is_drop_off: bool,
    pub max_hp: i32,
    #[serde(default)]
    pub produces: Vec<UnitType>,
    pub range: i32,
    #[serde(default)]
    pub requires: Vec<BuildingType>,
    pub storage_bonus: i32,
    pub vision: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnitStatsDto {
    #[serde(rename = "type")]
    pub kind: UnitType,
    pub armor: i32,
    pub attack_cooldown: i32,
    pub build_ticks: i32,
    pub cost: i32,
    pub damage: i32,
    pub max_hp: i32,
    pub produced_at: BuildingType,
    pub range: i32,
    pub speed: i32,
    pub vision: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeStatsDto {
    #[serde(rename = "type")]
    pub kind: UpgradeType,
    pub cost: i32,
    pub effect: String,
    pub requires: Option<UpgradeType>,
    pub research_ticks: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MapDto {
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub start_positions: Vec<PositionDto>,
    #[serde(default)]
    pub terrain: Vec<String>,
}

impl MapDto {
    pub fn is_passable(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return false;
        }
        self.terrain
            .get(y as usize)
            .and_then(|row| row.as_bytes().get(x as usize))
            .is_some_and(|&c| c != b'#')
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameStateDto {
    pub you: SelfDto,
    pub match_id: String,
    pub status: MatchStatus,
    pub tick: i32,
    pub max_ticks: i32,
    pub tick_interval_ms: i32,
    pub map_width: i32,
    pub map_height: i32,
    pub outcome: Option<MatchOutcomeDto>,
    #[serde(default)]
    pub units: Vec<UnitDto>,
    #[serde(default)]
    pub buildings: Vec<BuildingDto>,
    #[serde(default)]
    pub remembered_buildings: Vec<RememberedBuildingDto>,
    #[serde(default)]
    pub resources: Vec<ResourceDepositDto>,
    #[serde(default)]
    pub events: Vec<GameEventDto>,
    #[serde(default)]
    pub players: Vec<PlayerSummaryDto>,
    #[serde(default)]
    pub visibility: Vec<String>,
}

impl GameStateDto {
    pub fn my_units(&self) -> impl Iterator<Item = &UnitDto> {
        let slot = self.you.slot;
        self.units.iter().filter(move |u| u.owner == slot)
    }
    pub fn enemy_units(&self) -> impl Iterator<Item = &UnitDto> {
        let slot = self.you.slot;
        self.units.iter().filter(move |u| u.owner != slot)
    }
    pub fn my_buildings(&self) -> impl Iterator<Item = &BuildingDto> {
        let slot = self.you.slot;
        self.buildings.iter().filter(move |b| b.owner == slot)
    }
    pub fn enemy_buildings(&self) -> impl Iterator<Item = &BuildingDto> {
        let slot = self.you.slot;
        self.buildings.iter().filter(move |b| b.owner != slot)
    }
    pub fn is_visible(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        self.visibility
            .get(y as usize)
            .and_then(|row| row.as_bytes().get(x as usize))
            .is_some_and(|&c| c == b'1')
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SelfDto {
    pub player_id: String,
    pub name: String,
    pub slot: i32,
    pub resources: i32,
    pub storage_capacity: i32,
    pub income_per_minute: i32,
    pub queue_capacity: i32,
    pub queued_commands: i32,
    #[serde(default)]
    pub researching: Vec<ResearchDto>,
    #[serde(default)]
    pub upgrades: Vec<UpgradeType>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnitDto {
    pub id: i32,
    #[serde(rename = "type")]
    pub kind: UnitType,
    pub owner: i32,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub activity: UnitActivity,
    pub carrying: i32,
    pub destination: Option<PositionDto>,
    pub target_id: Option<i32>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BuildingDto {
    pub id: i32,
    #[serde(rename = "type")]
    pub kind: BuildingType,
    pub owner: i32,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub completed: bool,
    pub construction_percent: i32,
    #[serde(default)]
    pub production: Vec<ProductionDto>,
    pub rally: Option<PositionDto>,
    pub research: Option<ResearchDto>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RememberedBuildingDto {
    pub id: i32,
    #[serde(rename = "type")]
    pub kind: BuildingType,
    pub owner: i32,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub completed: bool,
    pub last_seen_tick: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProductionDto {
    pub unit_type: UnitType,
    pub percent: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResearchDto {
    pub building_id: i32,
    pub upgrade: UpgradeType,
    pub percent: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDepositDto {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub remaining: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameEventDto {
    pub tick: i32,
    pub kind: String,
    pub message: String,
    pub command_id: Option<i64>,
    pub entity_id: Option<i32>,
    pub owner: Option<i32>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSummaryDto {
    pub player_id: String,
    pub name: String,
    pub slot: i32,
    pub eliminated: bool,
    pub score: ScoreDto,
    pub start_position: PositionDto,
}

// ============================================================================
// Commands
// ============================================================================
/// - Move:     units + (x,y | tile)
/// - Attack:   units + (target_id | x,y | tile). A position means attack-move.
/// - Gather:   units + target_id (a resource deposit)
/// - Build:    units + building_type + (x,y | tile)
/// - Produce:  building_id + unit_type [+ count] [+ x,y | tile for rally]
/// - Rally:    building_id + (x,y | tile)
/// - Research: building_id + upgrade
/// - Stop:     units
/// Units are selected with `units` ("5-10", "1-7,12", "all", "idle") and/or
/// `unit_ids`, optionally narrowed by `unit_type`.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    #[serde(rename = "type")]
    pub kind: CommandType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_ids: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_type: Option<UnitType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub building_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub building_type: Option<BuildingType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<UpgradeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile: Option<i32>,
}

impl CommandRequest {
    pub fn new(kind: CommandType) -> Self {
        CommandRequest {
            kind,
            units: None,
            unit_ids: None,
            unit_type: None,
            building_id: None,
            building_type: None,
            target_id: None,
            upgrade: None,
            count: None,
            x: None,
            y: None,
            tile: None,
        }
    }

    pub fn move_to(unit_ids: Vec<i32>, x: i32, y: i32) -> Self {
        CommandRequest { unit_ids: Some(unit_ids), x: Some(x), y: Some(y), ..Self::new(CommandType::Move) }
    }

    pub fn attack(unit_ids: Vec<i32>, target_id: i32) -> Self {
        CommandRequest { unit_ids: Some(unit_ids), target_id: Some(target_id), ..Self::new(CommandType::Attack) }
    }

    pub fn attack_move(unit_ids: Vec<i32>, x: i32, y: i32) -> Self {
        CommandRequest { unit_ids: Some(unit_ids), x: Some(x), y: Some(y), ..Self::new(CommandType::Attack) }
    }

    pub fn gather(unit_ids: Vec<i32>, deposit_id: i32) -> Self {
        CommandRequest { unit_ids: Some(unit_ids), target_id: Some(deposit_id), ..Self::new(CommandType::Gather) }
    }

    pub fn build(unit_ids: Vec<i32>, building: BuildingType, x: i32, y: i32) -> Self {
        CommandRequest {
            unit_ids: Some(unit_ids),
            building_type: Some(building),
            x: Some(x),
            y: Some(y),
            ..Self::new(CommandType::Build)
        }
    }

    pub fn produce(building_id: i32, unit: UnitType, count: i32) -> Self {
        CommandRequest {
            building_id: Some(building_id),
            unit_type: Some(unit),
            count: Some(count),
            ..Self::new(CommandType::Produce)
        }
    }

    pub fn rally(building_id: i32, x: i32, y: i32) -> Self {
        CommandRequest { building_id: Some(building_id), x: Some(x), y: Some(y), ..Self::new(CommandType::Rally) }
    }

    pub fn research(building_id: i32, upgrade: UpgradeType) -> Self {
        CommandRequest { building_id: Some(building_id), upgrade: Some(upgrade), ..Self::new(CommandType::Research) }
    }

    pub fn stop(unit_ids: Vec<i32>) -> Self {
        CommandRequest { unit_ids: Some(unit_ids), ..Self::new(CommandType::Stop) }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SubmitCommandsRequest {
    pub commands: Vec<CommandRequest>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubmitCommandsResponse {
    pub accepted: i32,
    pub rejected: i32,
    pub queue_size: i32,
    pub queue_capacity: i32,
    #[serde(default)]
    pub results: Vec<CommandResult>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub index: i32,
    pub accepted: bool,
    pub command_id: Option<i64>,
    pub error: Option<CommandError>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}