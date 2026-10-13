//! The meta model: rules (a condition and an action) grouped by state, as
//! the meta engine runs them and the `.met` and `.af` files hold them.

use crate::route::NavigationSettings;

/// What a rule's condition tests. The order is VTank's condition numbering
/// for the kinds a `.met` file holds; the client's own notifications as
/// edges come after, so the numbers of the kinds above never move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MetaConditionKind {
    /// Never true.
    Never,
    /// Always true.
    #[default]
    Always,
    /// Every child is true.
    All,
    /// Any child is true.
    Any,
    /// A chat line matches a pattern.
    ChatMessage,
    /// Free main-pack slots at most a number.
    PackSlotsLessThanOrEqual,
    /// Seconds in the current state at least a number.
    SecondsInStateGreaterThanOrEqual,
    /// The loaded route is empty.
    NavigationRouteEmpty,
    /// The character died.
    CharacterDeath,
    /// A vendor window is open.
    AnyVendorOpen,
    /// The vendor window closed.
    VendorClosed,
    /// An item's count in the inventory at most a number.
    InventoryItemCountLessThanOrEqual,
    /// An item's count in the inventory at least a number.
    InventoryItemCountGreaterThanOrEqual,
    /// Monsters matching a name within a distance, at least a number.
    MonsterNameCountWithinDistance,
    /// Monsters of a priority within a distance, at least a number.
    MonsterPriorityCountWithinDistance,
    /// Buffing is due.
    NeedToBuff,
    /// No monsters within a distance.
    NoMonstersWithinDistance,
    /// The character's landblock equals a number.
    LandblockEquals,
    /// The character's landcell equals a number.
    LandcellEquals,
    /// Portal space was entered.
    PortalspaceEntered,
    /// Portal space was left.
    PortalspaceExited,
    /// The child is false.
    Not,
    /// Seconds in the state, kept across meta restarts, at least a number.
    PersistentSecondsInStateGreaterThanOrEqual,
    /// Seconds left on a spell at least a number.
    TimeLeftOnSpellGreaterThanOrEqual,
    /// Burden percent at least a number.
    BurdenPercentGreaterThanOrEqual,
    /// Distance from the nearest route point at least a number.
    DistanceFromAnyRoutePointGreaterThanOrEqual,
    /// An expression is true.
    Expression,
    /// A chat line matches a pattern; its groups are captured.
    ChatMessageCapture,
    /// The login completed (an edge).
    LoginComplete,
    /// The character logs off (an edge).
    Logoff,
    /// A portal transition ended (an edge).
    PortalTransition,
    /// An item use completed (an edge).
    ItemUseCompleted,
    /// A container opened (an edge).
    ContainerOpened,
    /// A container closed (an edge).
    ContainerClosed,
    /// A confirmation was asked (an edge).
    ConfirmationRequested,
}

impl MetaConditionKind {
    /// Every kind, in order.
    pub const ALL: [Self; 35] = [
        Self::Never,
        Self::Always,
        Self::All,
        Self::Any,
        Self::ChatMessage,
        Self::PackSlotsLessThanOrEqual,
        Self::SecondsInStateGreaterThanOrEqual,
        Self::NavigationRouteEmpty,
        Self::CharacterDeath,
        Self::AnyVendorOpen,
        Self::VendorClosed,
        Self::InventoryItemCountLessThanOrEqual,
        Self::InventoryItemCountGreaterThanOrEqual,
        Self::MonsterNameCountWithinDistance,
        Self::MonsterPriorityCountWithinDistance,
        Self::NeedToBuff,
        Self::NoMonstersWithinDistance,
        Self::LandblockEquals,
        Self::LandcellEquals,
        Self::PortalspaceEntered,
        Self::PortalspaceExited,
        Self::Not,
        Self::PersistentSecondsInStateGreaterThanOrEqual,
        Self::TimeLeftOnSpellGreaterThanOrEqual,
        Self::BurdenPercentGreaterThanOrEqual,
        Self::DistanceFromAnyRoutePointGreaterThanOrEqual,
        Self::Expression,
        Self::ChatMessageCapture,
        Self::LoginComplete,
        Self::Logoff,
        Self::PortalTransition,
        Self::ItemUseCompleted,
        Self::ContainerOpened,
        Self::ContainerClosed,
        Self::ConfirmationRequested,
    ];

    /// The C# member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Never => "Never",
            Self::Always => "Always",
            Self::All => "All",
            Self::Any => "Any",
            Self::ChatMessage => "ChatMessage",
            Self::PackSlotsLessThanOrEqual => "PackSlotsLessThanOrEqual",
            Self::SecondsInStateGreaterThanOrEqual => "SecondsInStateGreaterThanOrEqual",
            Self::NavigationRouteEmpty => "NavigationRouteEmpty",
            Self::CharacterDeath => "CharacterDeath",
            Self::AnyVendorOpen => "AnyVendorOpen",
            Self::VendorClosed => "VendorClosed",
            Self::InventoryItemCountLessThanOrEqual => "InventoryItemCountLessThanOrEqual",
            Self::InventoryItemCountGreaterThanOrEqual => "InventoryItemCountGreaterThanOrEqual",
            Self::MonsterNameCountWithinDistance => "MonsterNameCountWithinDistance",
            Self::MonsterPriorityCountWithinDistance => "MonsterPriorityCountWithinDistance",
            Self::NeedToBuff => "NeedToBuff",
            Self::NoMonstersWithinDistance => "NoMonstersWithinDistance",
            Self::LandblockEquals => "LandblockEquals",
            Self::LandcellEquals => "LandcellEquals",
            Self::PortalspaceEntered => "PortalspaceEntered",
            Self::PortalspaceExited => "PortalspaceExited",
            Self::Not => "Not",
            Self::PersistentSecondsInStateGreaterThanOrEqual => {
                "PersistentSecondsInStateGreaterThanOrEqual"
            }
            Self::TimeLeftOnSpellGreaterThanOrEqual => "TimeLeftOnSpellGreaterThanOrEqual",
            Self::BurdenPercentGreaterThanOrEqual => "BurdenPercentGreaterThanOrEqual",
            Self::DistanceFromAnyRoutePointGreaterThanOrEqual => {
                "DistanceFromAnyRoutePointGreaterThanOrEqual"
            }
            Self::Expression => "Expression",
            Self::ChatMessageCapture => "ChatMessageCapture",
            Self::LoginComplete => "LoginComplete",
            Self::Logoff => "Logoff",
            Self::PortalTransition => "PortalTransition",
            Self::ItemUseCompleted => "ItemUseCompleted",
            Self::ContainerOpened => "ContainerOpened",
            Self::ContainerClosed => "ContainerClosed",
            Self::ConfirmationRequested => "ConfirmationRequested",
        }
    }
}

/// What a rule's action does. The order is VTank's action numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MetaActionKind {
    /// Nothing.
    #[default]
    None,
    /// Change state.
    SetMetaState,
    /// Submit a chat line.
    ChatCommand,
    /// Every child action.
    All,
    /// Load the embedded route.
    LoadEmbeddedNavigationRoute,
    /// Call a state, to return to another.
    CallMetaState,
    /// Return from a call.
    ReturnFromCall,
    /// Run an expression.
    ExpressionAction,
    /// Submit an expression's result as a chat line.
    ChatExpression,
    /// Set the stuck watchdog.
    SetWatchdog,
    /// Clear the watchdog.
    ClearWatchdog,
    /// Read a VTank option into a variable.
    GetVtankOption,
    /// Set a VTank option from an expression.
    SetVtankOption,
    /// Create a view.
    CreateView,
    /// Destroy a view.
    DestroyView,
    /// Destroy every view.
    DestroyAllViews,
}

impl MetaActionKind {
    /// Every kind, in order.
    pub const ALL: [Self; 16] = [
        Self::None,
        Self::SetMetaState,
        Self::ChatCommand,
        Self::All,
        Self::LoadEmbeddedNavigationRoute,
        Self::CallMetaState,
        Self::ReturnFromCall,
        Self::ExpressionAction,
        Self::ChatExpression,
        Self::SetWatchdog,
        Self::ClearWatchdog,
        Self::GetVtankOption,
        Self::SetVtankOption,
        Self::CreateView,
        Self::DestroyView,
        Self::DestroyAllViews,
    ];

    /// The C# member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::SetMetaState => "SetMetaState",
            Self::ChatCommand => "ChatCommand",
            Self::All => "All",
            Self::LoadEmbeddedNavigationRoute => "LoadEmbeddedNavigationRoute",
            Self::CallMetaState => "CallMetaState",
            Self::ReturnFromCall => "ReturnFromCall",
            Self::ExpressionAction => "ExpressionAction",
            Self::ChatExpression => "ChatExpression",
            Self::SetWatchdog => "SetWatchdog",
            Self::ClearWatchdog => "ClearWatchdog",
            Self::GetVtankOption => "GetVtankOption",
            Self::SetVtankOption => "SetVtankOption",
            Self::CreateView => "CreateView",
            Self::DestroyView => "DestroyView",
            Self::DestroyAllViews => "DestroyAllViews",
        }
    }
}

/// A condition: its kind, the values it uses, and its children (`All`,
/// `Any`, `Not`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetaCondition {
    /// What it tests.
    pub kind: MetaConditionKind,
    /// Its text (pattern, item name, expression).
    pub text: String,
    /// Its second text (a colour list).
    pub secondary_text: String,
    /// Its number (count, seconds, distance, landblock, spell id).
    pub number: f64,
    /// Its second number (distance, seconds).
    pub secondary_number: f64,
    /// Its third number (a monster priority).
    pub tertiary_number: f64,
    /// Its children.
    pub children: Vec<MetaCondition>,
}

impl MetaCondition {
    /// A condition of `kind` with nothing else set.
    #[must_use]
    pub fn of(kind: MetaConditionKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// `Always`.
    #[must_use]
    pub fn always() -> Self {
        Self::of(MetaConditionKind::Always)
    }
}

/// An action: its kind, the values it uses, its children (`All`) and an
/// embedded route.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetaAction {
    /// What it does.
    pub kind: MetaActionKind,
    /// Its text (state, chat line, expression, option, view name).
    pub text: String,
    /// Its second text (return state, value, view XML, route name).
    pub secondary_text: String,
    /// Its number (a watchdog's distance).
    pub number: f64,
    /// Its second number (a watchdog's seconds).
    pub secondary_number: f64,
    /// Its children.
    pub children: Vec<MetaAction>,
    /// The route a `LoadEmbeddedNavigationRoute` loads.
    pub embedded_route: Option<Box<NavigationSettings>>,
}

impl MetaAction {
    /// An action of `kind` with nothing else set.
    #[must_use]
    pub fn of(kind: MetaActionKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }
}

/// The state a rule is in when none is named.
pub const DEFAULT_STATE: &str = "Default";

/// One rule.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaRule {
    /// The state it belongs to.
    pub state: String,
    /// When it fires.
    pub condition: MetaCondition,
    /// What it does.
    pub action: MetaAction,
    /// Switched off rules never fire.
    pub enabled: bool,
}

impl Default for MetaRule {
    fn default() -> Self {
        Self {
            state: DEFAULT_STATE.to_owned(),
            condition: MetaCondition::always(),
            action: MetaAction::default(),
            enabled: true,
        }
    }
}

impl MetaRule {
    /// A rule in `state`.
    #[must_use]
    pub fn new(state: &str, condition: MetaCondition, action: MetaAction) -> Self {
        Self {
            state: state.to_owned(),
            condition,
            action,
            enabled: true,
        }
    }
}

/// A whole meta: its rules in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetaProfile {
    /// The rules.
    pub rules: Vec<MetaRule>,
}
