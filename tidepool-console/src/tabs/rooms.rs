//! The rooms tab: the member's chatrooms, cursor-paged history, live messages,
//! presence, the composer and the session relay follower.
//!
//! The tab owns no network code. [`RoomsClient`] is the whole surface it needs,
//! and [`RoomsState::drive`] performs the room-list refresh, the open room's
//! history and member polls, the pending send and the relay's backfill.
//!
//! # History uses the forward cursor in both directions
//!
//! `GET /v1/chatrooms/{room}/messages?after=` is forward-only: a read returns
//! messages whose sequence is greater than the cursor. Opening a room therefore
//! pages forward from the beginning until a page reports no `next`, keeps the
//! newest [`ROOM_WINDOW`] messages and marks the rest as older. Scrolling to the
//! top of the transcript reads the page immediately below the oldest loaded
//! message by walking the same forward cursor backwards a page at a time
//! ([`older_messages_page`]); every merge is keyed by the server's message id,
//! so a page that overlaps the window can never duplicate a message.
//!
//! # Live means "read after the head"
//!
//! The console's blocking client has no socket dependency, so a message that
//! arrives while the room is open is found by reading forward from the highest
//! sequence already held, once a second. The message view is stored exactly as
//! the pool returned it, so its server id and sequence stay with it.
//!
//! # The relay marks where backfill stops
//!
//! Following a published session reads `GET /v1/sessions/{session}/relay/events`
//! from the cursor. Every event read while the follower is still catching up is
//! [`Origin::Backfill`]; the first read after a page reports no `next` is the
//! joining point, and every event from then on is [`Origin::Live`]. The view
//! keeps both sets in one sequence-ordered list and [`RelayFollow::live_from`]
//! names the first live row, so the screen can draw the boundary explicitly
//! instead of leaving a reader to guess.
//!
//! # Composing is multi-line, sending is deliberate
//!
//! Enter in the composer inserts a newline. A message is sent with `Ctrl-S`,
//! which cannot be confused with the newline a person typed on purpose. The
//! composer keeps its text when a send fails, and the message the pool returns
//! is merged into the transcript by its server id.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tidepool_client::endpoints::{
    GetV1ChatroomsRoomMessagesQuery, GetV1SessionsSessionRelayEventsQuery,
};
use tidepool_client::types::{
    HistoryResponse, MessageView, RelayEvent, RelayPage, RelayState, RoomMemberView, RoomView,
    SendMessageBody,
};

use crate::app::Result;

/// How many messages one history page asks for.
pub const ROOM_PAGE_SIZE: u64 = 50;

/// How many of the newest messages one open room keeps without asking again.
pub const ROOM_WINDOW: usize = 200;

/// How many cursor steps one catch-up or older-page walk may take.
pub const MAX_BACKFILL_STEPS: usize = 100;

/// How often the open room reads forward from its head.
pub const LIVE_INTERVAL: Duration = Duration::from_secs(1);

/// How often the open room's member list is read.
pub const PRESENCE_INTERVAL: Duration = Duration::from_secs(3);

/// How often the member's room list is read.
pub const ROOMS_INTERVAL: Duration = Duration::from_secs(15);

/// How many relayed events one page asks for.
pub const RELAY_PAGE_SIZE: u64 = 100;

/// How often a followed session is read after its backfill ends.
pub const RELAY_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// How often a followed session's publication state is read.
pub const RELAY_STATE_INTERVAL: Duration = Duration::from_secs(5);

/// How many transcript lines one page key scrolls.
pub const SCROLL_PAGE: usize = 10;

/// What the rooms tab needs from a pool.
pub trait RoomsClient {
    /// List the rooms the signed-in member belongs to.
    fn list_rooms(&mut self) -> Result<Vec<RoomView>>;
    /// List one room's members, which is the presence this client can observe.
    fn room_members(&mut self, room: &str) -> Result<Vec<RoomMemberView>>;
    /// Read one page of a room's history after an exclusive sequence cursor.
    fn room_history(
        &mut self,
        room: &str,
        query: GetV1ChatroomsRoomMessagesQuery,
    ) -> Result<HistoryResponse>;
    /// Send one message to a room and return the pool's stored view.
    fn send_message(&mut self, room: &str, body: SendMessageBody) -> Result<MessageView>;
    /// Read one page of a session's relayed events after an exclusive cursor.
    fn relay_events(
        &mut self,
        session: &str,
        query: GetV1SessionsSessionRelayEventsQuery,
    ) -> Result<RelayPage>;
    /// Read whether a session is publishing its relay.
    fn relay_state(&mut self, session: &str) -> Result<RelayState>;
}

/// Order two messages by the server's sequence, then by id.
///
/// Timestamps are never compared; the sequence is the pool's own order and the
/// id is a deterministic tie-break.
pub fn compare_messages(left: &MessageView, right: &MessageView) -> Ordering {
    left.sequence
        .cmp(&right.sequence)
        .then_with(|| left.id.cmp(&right.id))
}

/// Merge two message lists, keyed by the server's id.
///
/// A message that arrives twice appears once and the newer copy wins, because
/// the pool is the source of truth for a body. The result is ordered by
/// sequence and id, so a page that overlaps the loaded window cannot duplicate
/// or reorder a message.
pub fn merge_messages(existing: &[MessageView], incoming: &[MessageView]) -> Vec<MessageView> {
    let mut by_id: HashMap<String, MessageView> = HashMap::with_capacity(existing.len());
    for message in existing {
        by_id.insert(message.id.clone(), message.clone());
    }
    for message in incoming {
        by_id.insert(message.id.clone(), message.clone());
    }
    let mut merged: Vec<MessageView> = by_id.into_values().collect();
    merged.sort_by(compare_messages);
    merged
}

/// The highest sequence held, which is the cursor for newer messages.
pub fn head_cursor(messages: &[MessageView]) -> Option<i64> {
    messages.iter().map(|message| message.sequence).max()
}

/// The lowest sequence held, which is the cursor for older messages.
pub fn tail_cursor(messages: &[MessageView]) -> Option<i64> {
    messages.iter().map(|message| message.sequence).min()
}

/// One page immediately older than a cursor, plus whether more may exist.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OlderPage {
    /// The newest `page_size` messages below the requested cursor, ascending.
    pub messages: Vec<MessageView>,
    /// True when a further older page may exist.
    pub has_older: bool,
}

/// Read the page immediately older than `before`.
///
/// The contract only reads forward from `after`, so the walk steps the cursor
/// back one page at a time and asks for `(cursor - page_size, …)`. Everything at
/// or above `before` is discarded and the rest is accumulated ascending. A short
/// read is not proof that the beginning was reached: with gaps in the sequence
/// the walk steps back again until a full page is collected or the room's start
/// is read. The returned page is the newest `page_size` messages below `before`,
/// so merging it can never duplicate a message the transcript already holds.
pub fn older_messages_page(
    client: &mut dyn RoomsClient,
    room: &str,
    before: i64,
    page_size: u64,
) -> Result<OlderPage> {
    let size = page_size.max(1);
    let size_i64 = i64::try_from(size).unwrap_or(i64::MAX);
    let mut cursor = before;
    let mut collected: Vec<MessageView> = Vec::new();
    let mut reached_start = false;

    for _ in 0..MAX_BACKFILL_STEPS {
        if cursor <= 0 {
            reached_start = true;
            break;
        }
        let window = cursor.saturating_sub(size_i64).max(0);
        let page = client.room_history(
            room,
            GetV1ChatroomsRoomMessagesQuery {
                after: Some(window as u64),
                limit: Some(size),
            },
        )?;
        let mut below: Vec<MessageView> = page
            .messages
            .into_iter()
            .filter(|message| message.sequence < cursor)
            .collect();
        below.sort_by(compare_messages);
        if below.is_empty() {
            if window == 0 {
                reached_start = true;
                break;
            }
            cursor = window;
            continue;
        }
        let oldest = below
            .first()
            .map(|message| message.sequence)
            .unwrap_or(window);
        let mut rebuilt = below;
        rebuilt.append(&mut collected);
        collected = rebuilt;
        cursor = oldest;
        if window == 0 {
            // Reading from the very beginning returned the room's oldest
            // message, so there is nothing older even when it is not sequence 1.
            reached_start = true;
        }
        if collected.len() as u64 >= size {
            break;
        }
    }

    let more_than_page = collected.len() as u64 > size;
    let messages = if more_than_page {
        collected.split_off(collected.len() - size as usize)
    } else {
        collected
    };
    Ok(OlderPage {
        messages,
        has_older: !reached_start || more_than_page,
    })
}

/// Which side of the relay's joining point an event arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Published before the follower caught up, read through the cursor.
    Backfill,
    /// Published after the follower reached the head.
    Live,
}

impl Origin {
    /// The lowercase word the relay pane prints for this origin.
    pub fn word(self) -> &'static str {
        match self {
            Origin::Backfill => "backfill",
            Origin::Live => "live",
        }
    }
}

/// One relayed event and the side of the joining point it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct RelayRow {
    /// The event exactly as the pool returned it, id and sequence included.
    pub event: RelayEvent,
    /// Where in the follow this event was read.
    pub origin: Origin,
}

/// What the relay follower is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowPhase {
    /// Reading forward through events published before the follow started.
    Backfilling,
    /// Caught up; each poll asks only for events after the head.
    Live,
}

impl FollowPhase {
    /// The lowercase word the relay pane prints for this phase.
    pub fn word(self) -> &'static str {
        match self {
            FollowPhase::Backfilling => "backfilling",
            FollowPhase::Live => "live",
        }
    }
}

/// The relay of one published session, as this console follows it.
#[derive(Debug)]
pub struct RelayFollow {
    session: String,
    rows: Vec<RelayRow>,
    cursor: Option<i64>,
    phase: FollowPhase,
    publication: Option<RelayState>,
    state_read: bool,
    last_state: Option<Instant>,
    last_poll: Option<Instant>,
    problem: Option<String>,
}

impl RelayFollow {
    /// A follower that has not read anything yet.
    fn new(session: String) -> Self {
        Self {
            session,
            rows: Vec::new(),
            cursor: None,
            phase: FollowPhase::Backfilling,
            publication: None,
            state_read: false,
            last_state: None,
            last_poll: None,
            problem: None,
        }
    }

    /// The session being followed.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// Every event read so far, ordered by sequence.
    pub fn rows(&self) -> &[RelayRow] {
        &self.rows
    }

    /// Where the follower is in the catch-up.
    pub fn phase(&self) -> FollowPhase {
        self.phase
    }

    /// The publication state the pool last reported, once it has answered.
    pub fn publication(&self) -> Option<&RelayState> {
        self.publication.as_ref()
    }

    /// Why the last relay read failed, when it did.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// The highest event sequence held, which is the next read's cursor.
    pub fn cursor(&self) -> Option<i64> {
        self.cursor
    }

    /// The index of the first live event: the joining point on screen.
    pub fn live_from(&self) -> Option<usize> {
        self.rows.iter().position(|row| row.origin == Origin::Live)
    }

    /// How many events came from the backfill.
    pub fn backfill_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.origin == Origin::Backfill)
            .count()
    }

    /// How many events arrived after the joining point.
    pub fn live_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.origin == Origin::Live)
            .count()
    }

    /// Merge one read into the follow, keyed by the event's server id.
    fn append(&mut self, events: Vec<RelayEvent>, origin: Origin) {
        for event in events {
            match self.rows.iter_mut().find(|row| row.event.id == event.id) {
                Some(row) => {
                    row.event = event;
                    row.origin = origin;
                }
                None => self.rows.push(RelayRow { event, origin }),
            }
        }
        self.rows.sort_by(|left, right| {
            left.event
                .sequence
                .cmp(&right.event.sequence)
                .then_with(|| left.event.id.cmp(&right.event.id))
        });
        if let Some(head) = self.rows.iter().map(|row| row.event.sequence).max() {
            self.cursor = Some(self.cursor.map_or(head, |cursor| cursor.max(head)));
        }
    }
}

/// One message being composed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composer {
    body: String,
}

impl Composer {
    /// An empty composer.
    pub fn new() -> Self {
        Self::default()
    }

    /// The text typed so far, newlines included.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Whether the message has no non-whitespace text.
    pub fn is_blank(&self) -> bool {
        self.body.trim().is_empty()
    }

    /// The message as the pool expects it.
    pub fn request(&self) -> SendMessageBody {
        SendMessageBody {
            body: self.body.clone(),
            reply_to: None,
        }
    }

    fn push(&mut self, character: char) {
        self.body.push(character);
    }

    fn newline(&mut self) {
        self.body.push('\n');
    }

    fn backspace(&mut self) {
        self.body.pop();
    }
}

/// The author's name as the transcript prints it.
pub fn author_label(message: &MessageView) -> String {
    match &message.author {
        Some(principal) if !principal.id.trim().is_empty() => match principal.kind.as_str() {
            "session" => format!("{} (session)", principal.id),
            _ => principal.id.clone(),
        },
        _ => "unknown author".to_owned(),
    }
}

/// One open room's transcript and its cursors.
#[derive(Debug)]
struct Transcript {
    room: String,
    messages: Vec<MessageView>,
    initial: bool,
    has_older: bool,
    pending_older: bool,
    loading_older: bool,
    history_problem: Option<String>,
    older_problem: Option<String>,
    last_poll: Option<Instant>,
    scroll: usize,
}

impl Transcript {
    fn new(room: String) -> Self {
        Self {
            room,
            messages: Vec::new(),
            initial: true,
            has_older: false,
            pending_older: false,
            loading_older: false,
            history_problem: None,
            older_problem: None,
            last_poll: None,
            scroll: 0,
        }
    }

    /// An estimate of the rendered transcript's line count.
    ///
    /// The renderer wraps text, so this is used only to decide when the top of
    /// the transcript has been reached, never to cut a message.
    fn line_count(&self) -> usize {
        if self.messages.is_empty() {
            return 1;
        }
        self.messages
            .iter()
            .map(|message| {
                let body = message.body.lines().count().max(1);
                body + 2
            })
            .sum()
    }
}

/// The whole rooms tab.
#[derive(Debug, Default)]
pub struct RoomsState {
    rooms: Vec<RoomView>,
    selected: usize,
    rooms_read: bool,
    rooms_problem: Option<String>,
    last_rooms: Option<Instant>,
    members: Vec<RoomMemberView>,
    members_read: bool,
    members_problem: Option<String>,
    last_members: Option<Instant>,
    transcript: Option<Transcript>,
    composer: Option<Composer>,
    pending_send: bool,
    relay: Option<RelayFollow>,
    follow_prompt: Option<String>,
    notice: Option<String>,
    member: Option<String>,
}

impl RoomsState {
    /// An empty tab: no rooms, no open transcript, no follow.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember the signed-in member, shown in the header and the send notice.
    pub fn set_member(&mut self, member: Option<String>) {
        self.member = member;
    }

    /// The signed-in member, when one is known.
    pub fn member(&self) -> Option<&str> {
        self.member.as_deref()
    }

    /// The rooms the pool last reported.
    pub fn rooms(&self) -> &[RoomView] {
        &self.rooms
    }

    /// The highlighted room in the list.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The highlighted room, when the list is not empty.
    pub fn selected_room(&self) -> Option<&RoomView> {
        self.rooms.get(self.selected)
    }

    /// Why the last room-list read failed, when it did.
    pub fn rooms_problem(&self) -> Option<&str> {
        self.rooms_problem.as_deref()
    }

    /// The open room's public id.
    pub fn open_room_id(&self) -> Option<&str> {
        self.transcript
            .as_ref()
            .map(|transcript| transcript.room.as_str())
    }

    /// The open room's messages, oldest first.
    pub fn messages(&self) -> &[MessageView] {
        self.transcript
            .as_ref()
            .map(|transcript| transcript.messages.as_slice())
            .unwrap_or(&[])
    }

    /// Whether an older page may still exist.
    pub fn has_older(&self) -> bool {
        self.transcript
            .as_ref()
            .is_some_and(|transcript| transcript.has_older)
    }

    /// Whether an older page is being read.
    pub fn loading_older(&self) -> bool {
        self.transcript
            .as_ref()
            .is_some_and(|transcript| transcript.loading_older)
    }

    /// Why the open room's initial history read failed, when it did.
    pub fn history_problem(&self) -> Option<&str> {
        self.transcript
            .as_ref()
            .and_then(|transcript| transcript.history_problem.as_deref())
    }

    /// Why the last older-page read failed, when it did.
    pub fn older_problem(&self) -> Option<&str> {
        self.transcript
            .as_ref()
            .and_then(|transcript| transcript.older_problem.as_deref())
    }

    /// How many rendered lines the reader has scrolled up from the bottom.
    pub fn scroll(&self) -> usize {
        self.transcript
            .as_ref()
            .map(|transcript| transcript.scroll)
            .unwrap_or(0)
    }

    /// The estimate of the open transcript's rendered line count.
    pub fn transcript_lines(&self) -> usize {
        self.transcript
            .as_ref()
            .map(Transcript::line_count)
            .unwrap_or(0)
    }

    /// The open room's members, which is the presence the pool exposes.
    pub fn members(&self) -> &[RoomMemberView] {
        &self.members
    }

    /// Why the last member read failed, when it did.
    pub fn members_problem(&self) -> Option<&str> {
        self.members_problem.as_deref()
    }

    /// The composer, while it is on screen.
    pub fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    /// The relay being followed, while one is.
    pub fn relay(&self) -> Option<&RelayFollow> {
        self.relay.as_ref()
    }

    /// The session-id prompt the `f` key opens, while it is open.
    pub fn follow_prompt(&self) -> Option<&str> {
        self.follow_prompt.as_deref()
    }

    /// The last action's outcome, when it was not a plain success.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Open one room without waiting for a key.
    pub fn open_room(&mut self, room: impl Into<String>) {
        self.transcript = Some(Transcript::new(room.into()));
        self.composer = None;
        self.pending_send = false;
        self.members.clear();
        self.members_read = false;
        self.members_problem = None;
        self.last_members = None;
        self.notice = None;
    }

    /// Close the open room; the relay follow is independent and stays open.
    pub fn close_room(&mut self) {
        self.transcript = None;
        self.composer = None;
        self.pending_send = false;
        self.members.clear();
        self.members_read = false;
        self.members_problem = None;
        self.last_members = None;
    }

    /// Follow one published session's relay from its first event.
    pub fn follow_session(&mut self, session: impl Into<String>) {
        let session = session.into();
        self.relay = Some(RelayFollow::new(session));
        self.follow_prompt = None;
    }

    /// Stop following the current session.
    pub fn stop_follow(&mut self) {
        self.relay = None;
    }

    /// Move the room highlight down, wrapping.
    pub fn select_next(&mut self) {
        let count = self.rooms.len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % count;
    }

    /// Move the room highlight up, wrapping.
    pub fn select_previous(&mut self) {
        let count = self.rooms.len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + count - 1) % count;
    }

    /// Handle one key press. Returns whether the tab consumed it.
    ///
    /// While the composer or the follow prompt is open every key belongs to it,
    /// so `q` types a `q` instead of quitting the console.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if self.composer.is_some() {
            return self.on_composer_key(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        if self.follow_prompt.is_some() {
            return self.on_prompt_key(key);
        }
        if self.transcript.is_some() {
            return self.on_room_key(key);
        }
        self.on_list_key(key)
    }

    fn on_prompt_key(&mut self, key: KeyEvent) -> bool {
        let Some(prompt) = self.follow_prompt.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.follow_prompt = None,
            KeyCode::Enter => {
                let session = prompt.trim().to_owned();
                if session.is_empty() {
                    self.follow_prompt = None;
                    self.notice = Some("Name a session id to follow.".to_owned());
                } else {
                    self.follow_session(session);
                    self.notice = Some("Following the session's relay.".to_owned());
                }
            }
            KeyCode::Backspace => {
                prompt.pop();
            }
            KeyCode::Char(character) => prompt.push(character),
            _ => {}
        }
        true
    }

    fn on_list_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                true
            }
            KeyCode::Enter => {
                if let Some(room) = self.selected_room().map(|room| room.id.clone()) {
                    self.open_room(room);
                }
                true
            }
            KeyCode::Char('f') => {
                self.follow_prompt = Some(String::new());
                true
            }
            KeyCode::Char('x') => {
                self.stop_follow();
                self.notice = Some("Stopped following the relay.".to_owned());
                true
            }
            KeyCode::Char('r') => {
                self.rooms_read = false;
                self.last_rooms = None;
                true
            }
            _ => false,
        }
    }

    fn on_room_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.close_room();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down(1);
                true
            }
            KeyCode::PageUp => {
                self.scroll_up(SCROLL_PAGE);
                true
            }
            KeyCode::PageDown => {
                self.scroll_down(SCROLL_PAGE);
                true
            }
            KeyCode::Char('c') => {
                self.composer = Some(Composer::new());
                true
            }
            KeyCode::Char('f') => {
                self.follow_prompt = Some(String::new());
                true
            }
            KeyCode::Char('x') => {
                self.stop_follow();
                self.notice = Some("Stopped following the relay.".to_owned());
                true
            }
            KeyCode::Char('r') => {
                if let Some(transcript) = self.transcript.as_mut() {
                    transcript.initial = true;
                    transcript.history_problem = None;
                    transcript.older_problem = None;
                }
                true
            }
            _ => false,
        }
    }

    fn on_composer_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('s') {
                self.pending_send = true;
            }
            return true;
        }
        let Some(composer) = self.composer.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc => self.composer = None,
            KeyCode::Enter => composer.newline(),
            KeyCode::Backspace => composer.backspace(),
            KeyCode::Char(character) => composer.push(character),
            _ => {}
        }
        true
    }

    /// Scroll up and, at the top, ask for the page below the oldest message.
    fn scroll_up(&mut self, amount: usize) {
        let Some(transcript) = self.transcript.as_mut() else {
            return;
        };
        let top = transcript.line_count().saturating_sub(1);
        transcript.scroll = (transcript.scroll + amount).min(top);
        if transcript.has_older && transcript.scroll >= top {
            transcript.pending_older = true;
        }
    }

    fn scroll_down(&mut self, amount: usize) {
        if let Some(transcript) = self.transcript.as_mut() {
            transcript.scroll = transcript.scroll.saturating_sub(amount);
        }
    }

    /// Refresh the room list, the open transcript, the pending send and the
    /// relay on their timers.
    ///
    /// The console calls this from its event loop, so new messages, member
    /// changes and relayed events arrive without a key press.
    pub fn drive(&mut self, client: &mut dyn RoomsClient, now: Instant) {
        self.refresh_rooms(client, now);
        self.poll_transcript(client, now);
        self.poll_members(client, now);
        self.drain_send(client);
        self.poll_relay(client, now);
    }

    fn refresh_rooms(&mut self, client: &mut dyn RoomsClient, now: Instant) {
        let due = !self.rooms_read
            || self
                .last_rooms
                .is_none_or(|last| now.saturating_duration_since(last) >= ROOMS_INTERVAL);
        if !due {
            return;
        }
        self.last_rooms = Some(now);
        self.rooms_read = true;
        match client.list_rooms() {
            Ok(rooms) => {
                self.rooms = rooms;
                self.rooms_problem = None;
                if self.rooms.is_empty() {
                    self.selected = 0;
                } else {
                    self.selected = self.selected.min(self.rooms.len() - 1);
                }
            }
            Err(error) => self.rooms_problem = Some(error.to_string()),
        }
    }

    fn poll_transcript(&mut self, client: &mut dyn RoomsClient, now: Instant) {
        let Some(transcript) = self.transcript.as_mut() else {
            return;
        };
        if transcript.initial {
            transcript.initial = false;
            Self::load_initial_history(client, transcript, now);
        }
        if transcript.pending_older && !transcript.loading_older && transcript.has_older {
            transcript.pending_older = false;
            transcript.loading_older = true;
            Self::load_older(client, transcript);
        }
        let due = transcript
            .last_poll
            .is_none_or(|last| now.saturating_duration_since(last) >= LIVE_INTERVAL);
        if due {
            transcript.last_poll = Some(now);
            Self::poll_new_messages(client, transcript);
        }
    }

    fn load_initial_history(
        client: &mut dyn RoomsClient,
        transcript: &mut Transcript,
        now: Instant,
    ) {
        let room = transcript.room.clone();
        let mut cursor: Option<i64> = None;
        let mut collected: Vec<MessageView> = Vec::new();
        let mut problem = None;
        for _ in 0..MAX_BACKFILL_STEPS {
            let query = GetV1ChatroomsRoomMessagesQuery {
                after: cursor.map(|value| value.max(0) as u64),
                limit: Some(ROOM_PAGE_SIZE),
            };
            match client.room_history(&room, query) {
                Ok(page) => {
                    if page.messages.is_empty() {
                        break;
                    }
                    collected = merge_messages(&collected, &page.messages);
                    match page.next {
                        Some(next) if Some(next) != cursor => cursor = Some(next),
                        _ => break,
                    }
                }
                Err(error) => {
                    problem = Some(error.to_string());
                    break;
                }
            }
        }

        let mut truncated = false;
        if collected.len() > ROOM_WINDOW {
            truncated = true;
            let discard = collected.len() - ROOM_WINDOW;
            collected.drain(..discard);
        }
        transcript.messages = merge_messages(&transcript.messages, &collected);
        transcript.has_older = truncated;
        transcript.history_problem = problem;
        transcript.older_problem = None;
        transcript.last_poll = Some(now);
        transcript.scroll = 0;
    }

    fn load_older(client: &mut dyn RoomsClient, transcript: &mut Transcript) {
        let Some(before) = tail_cursor(&transcript.messages) else {
            transcript.loading_older = false;
            transcript.has_older = false;
            return;
        };
        let room = transcript.room.clone();
        match older_messages_page(client, &room, before, ROOM_PAGE_SIZE) {
            Ok(page) => {
                let before_lines = transcript.line_count();
                transcript.messages = merge_messages(&transcript.messages, &page.messages);
                let grew = transcript.line_count().saturating_sub(before_lines);
                transcript.has_older = page.has_older && !page.messages.is_empty();
                transcript.older_problem = None;
                transcript.loading_older = false;
                if transcript.scroll > 0 {
                    transcript.scroll += grew;
                }
            }
            Err(error) => {
                transcript.older_problem = Some(error.to_string());
                transcript.loading_older = false;
            }
        }
    }

    fn poll_new_messages(client: &mut dyn RoomsClient, transcript: &mut Transcript) {
        let room = transcript.room.clone();
        let mut cursor = head_cursor(&transcript.messages);
        for _ in 0..MAX_BACKFILL_STEPS {
            let query = GetV1ChatroomsRoomMessagesQuery {
                after: cursor.map(|value| value.max(0) as u64),
                limit: Some(ROOM_PAGE_SIZE),
            };
            match client.room_history(&room, query) {
                Ok(page) => {
                    if page.messages.is_empty() {
                        transcript.history_problem = None;
                        break;
                    }
                    let before_lines = transcript.line_count();
                    transcript.messages = merge_messages(&transcript.messages, &page.messages);
                    if transcript.scroll > 0 {
                        transcript.scroll += transcript.line_count().saturating_sub(before_lines);
                    }
                    transcript.history_problem = None;
                    match page.next {
                        Some(next) if Some(next) != cursor => cursor = Some(next),
                        _ => break,
                    }
                }
                Err(error) => {
                    transcript.history_problem = Some(error.to_string());
                    break;
                }
            }
        }
    }

    fn poll_members(&mut self, client: &mut dyn RoomsClient, now: Instant) {
        let Some(transcript) = self.transcript.as_ref() else {
            return;
        };
        let room = transcript.room.clone();
        let due = !self.members_read
            || self
                .last_members
                .is_none_or(|last| now.saturating_duration_since(last) >= PRESENCE_INTERVAL);
        if !due {
            return;
        }
        self.last_members = Some(now);
        self.members_read = true;
        match client.room_members(&room) {
            Ok(members) => {
                self.members = members;
                self.members_problem = None;
            }
            Err(error) => self.members_problem = Some(error.to_string()),
        }
    }

    fn drain_send(&mut self, client: &mut dyn RoomsClient) {
        if !self.pending_send {
            return;
        }
        self.pending_send = false;
        let Some(room) = self.open_room_id().map(str::to_owned) else {
            return;
        };
        let Some(composer) = self.composer.as_ref() else {
            return;
        };
        if composer.is_blank() {
            self.notice = Some("Write a message before sending.".to_owned());
            return;
        }
        let request = composer.request();
        match client.send_message(&room, request) {
            Ok(message) => {
                if let Some(transcript) = self.transcript.as_mut() {
                    transcript.messages =
                        merge_messages(&transcript.messages, std::slice::from_ref(&message));
                    transcript.scroll = 0;
                }
                let who = self
                    .member
                    .clone()
                    .unwrap_or_else(|| "the signed-in member".to_owned());
                self.notice = Some(format!(
                    "Sent to {room} as {who}; the pool id is {}.",
                    message.id
                ));
                self.composer = None;
            }
            Err(error) => {
                self.notice = Some(format!("send failed: {error}"));
            }
        }
    }

    fn poll_relay(&mut self, client: &mut dyn RoomsClient, now: Instant) {
        let Some(follow) = self.relay.as_mut() else {
            return;
        };
        let state_due = !follow.state_read
            || follow
                .last_state
                .is_none_or(|last| now.saturating_duration_since(last) >= RELAY_STATE_INTERVAL);
        if state_due {
            follow.last_state = Some(now);
            match client.relay_state(&follow.session) {
                Ok(publication) => {
                    follow.publication = Some(publication);
                    follow.state_read = true;
                }
                Err(error) => {
                    follow.state_read = true;
                    follow.problem = Some(format!("relay state: {error}"));
                }
            }
        }

        match follow.phase {
            FollowPhase::Backfilling => {
                let retry = if follow.problem.is_some() {
                    RELAY_POLL_INTERVAL
                } else {
                    Duration::ZERO
                };
                let due = follow
                    .last_poll
                    .is_none_or(|last| now.saturating_duration_since(last) >= retry);
                if !due {
                    return;
                }
                follow.last_poll = Some(now);
                let query = GetV1SessionsSessionRelayEventsQuery {
                    after: follow.cursor.map(|value| value.max(0) as u64),
                    limit: Some(RELAY_PAGE_SIZE),
                };
                match client.relay_events(&follow.session, query) {
                    Ok(page) => {
                        let more = page.next.is_some();
                        follow.append(page.events, Origin::Backfill);
                        follow.problem = None;
                        if !more {
                            follow.phase = FollowPhase::Live;
                        }
                    }
                    Err(error) => follow.problem = Some(error.to_string()),
                }
            }
            FollowPhase::Live => {
                let due = follow
                    .last_poll
                    .is_none_or(|last| now.saturating_duration_since(last) >= RELAY_POLL_INTERVAL);
                if !due {
                    return;
                }
                follow.last_poll = Some(now);
                for _ in 0..MAX_BACKFILL_STEPS {
                    let query = GetV1SessionsSessionRelayEventsQuery {
                        after: follow.cursor.map(|value| value.max(0) as u64),
                        limit: Some(RELAY_PAGE_SIZE),
                    };
                    match client.relay_events(&follow.session, query) {
                        Ok(page) => {
                            let more = page.next.is_some();
                            follow.append(page.events, Origin::Live);
                            follow.problem = None;
                            if !more {
                                break;
                            }
                        }
                        Err(error) => {
                            follow.problem = Some(error.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::{HashMap, VecDeque};

    use serde_json::Value;
    use tidepool_client::endpoints::{
        GetV1ChatroomsRoomMessagesQuery, GetV1SessionsSessionRelayEventsQuery,
    };
    use tidepool_client::types::{
        HistoryResponse, MessageView, Principal, RelayEvent, RelayPage, RelayState, RoomMemberView,
        RoomView, SendMessageBody,
    };

    use super::{RELAY_PAGE_SIZE, RoomsClient};
    use crate::app::Result;
    use crate::app::testing::transport;

    /// One room with a name and a visibility.
    pub(crate) fn room(id: &str, name: &str) -> RoomView {
        RoomView {
            id: id.to_owned(),
            name: name.to_owned(),
            visibility: "public".to_owned(),
            origin_pool: None,
            created_at: "2026-10-29T00:00:00Z".to_owned(),
            updated_at: "2026-10-29T00:00:00Z".to_owned(),
        }
    }

    /// One message with a sequence, server id, author and body.
    pub(crate) fn message(sequence: i64, id: &str, author: &str, body: &str) -> MessageView {
        MessageView {
            id: id.to_owned(),
            sequence,
            body: body.to_owned(),
            created_at: format!("2026-10-29T00:00:{:02}Z", sequence.rem_euclid(60)),
            project: "proj_1".to_owned(),
            author: Some(Principal {
                id: author.to_owned(),
                kind: "user".to_owned(),
            }),
            reply_to: None,
        }
    }

    /// One room member.
    pub(crate) fn member(id: &str, role: &str, kind: &str) -> RoomMemberView {
        RoomMemberView {
            id: id.to_owned(),
            role: role.to_owned(),
            principal: Principal {
                id: id.to_owned(),
                kind: kind.to_owned(),
            },
            joined_at: "2026-10-29T00:00:00Z".to_owned(),
        }
    }

    /// One relayed event.
    pub(crate) fn relay_event(sequence: i64, kind: &str, payload: Value) -> RelayEvent {
        RelayEvent {
            id: format!("evt-{sequence}"),
            sequence,
            kind: kind.to_owned(),
            payload,
            session: "sess-1".to_owned(),
            created_at: format!("2026-10-29T00:00:{:02}Z", sequence.rem_euclid(60)),
        }
    }

    /// One publication state.
    pub(crate) fn relay_state(session: &str, enabled: bool) -> RelayState {
        RelayState {
            session: session.to_owned(),
            enabled,
            started_at: Some("2026-10-29T00:00:00Z".to_owned()),
            stopped_at: None,
        }
    }

    /// A client that emulates the pool's cursor reads over an in-memory log.
    #[derive(Debug, Default)]
    pub(crate) struct ScriptedRooms {
        /// Queued room lists.
        pub(crate) room_lists: VecDeque<Result<Vec<RoomView>>>,
        /// Queued member lists.
        pub(crate) member_lists: VecDeque<Result<Vec<RoomMemberView>>>,
        /// The room histories, keyed by public room id.
        pub(crate) history: HashMap<String, Vec<MessageView>>,
        /// Queued answers to a send.
        pub(crate) sends: VecDeque<Result<MessageView>>,
        /// The relayed events, keyed by session id.
        pub(crate) relay: HashMap<String, Vec<RelayEvent>>,
        /// The publication states, keyed by session id.
        pub(crate) relay_states: HashMap<String, RelayState>,
        /// A client-side page cap so a test can force several relay pages.
        pub(crate) relay_page_limit: Option<usize>,
        /// A failure forced onto every history read.
        pub(crate) history_problem: Option<String>,
        /// Every body sent, in order.
        pub(crate) sent: Vec<(String, SendMessageBody)>,
        /// Every call served, in order.
        pub(crate) calls: Vec<String>,
    }

    impl ScriptedRooms {
        /// An empty script.
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// Queue one room list for the next read.
        pub(crate) fn queue_rooms(&mut self, rooms: Vec<RoomView>) {
            self.room_lists.push_back(Ok(rooms));
        }

        /// Queue one member list for the next read.
        pub(crate) fn queue_members(&mut self, members: Vec<RoomMemberView>) {
            self.member_lists.push_back(Ok(members));
        }

        /// Replace one room's history.
        pub(crate) fn set_history(&mut self, room: &str, messages: Vec<MessageView>) {
            self.history.insert(room.to_owned(), messages);
        }

        /// Append one message to a room's history.
        pub(crate) fn push_message(&mut self, room: &str, message: MessageView) {
            self.history
                .entry(room.to_owned())
                .or_default()
                .push(message);
        }

        /// Queue one answer to the next send.
        pub(crate) fn queue_send(&mut self, message: MessageView) {
            self.sends.push_back(Ok(message));
        }

        /// Append one relayed event to a session's log.
        pub(crate) fn push_relay(&mut self, session: &str, event: RelayEvent) {
            self.relay
                .entry(session.to_owned())
                .or_default()
                .push(event);
        }

        /// Set one session's publication state.
        pub(crate) fn set_relay_state(&mut self, state: RelayState) {
            self.relay_states.insert(state.session.clone(), state);
        }
    }

    impl RoomsClient for ScriptedRooms {
        fn list_rooms(&mut self) -> Result<Vec<RoomView>> {
            self.calls.push("rooms".to_owned());
            self.room_lists
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))
        }

        fn room_members(&mut self, room: &str) -> Result<Vec<RoomMemberView>> {
            self.calls.push(format!("members:{room}"));
            self.member_lists
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))
        }

        fn room_history(
            &mut self,
            room: &str,
            query: GetV1ChatroomsRoomMessagesQuery,
        ) -> Result<HistoryResponse> {
            let after = query.after.unwrap_or(0);
            let limit = query.limit.unwrap_or(50).max(1);
            self.calls.push(format!("history:{room}:{after}:{limit}"));
            if let Some(problem) = &self.history_problem {
                return Err(transport(problem));
            }
            let all = self.history.get(room).cloned().unwrap_or_default();
            let mut remaining: Vec<MessageView> = all
                .into_iter()
                .filter(|message| message.sequence.max(0) as u64 > after)
                .collect();
            remaining.sort_by(super::compare_messages);
            let page: Vec<MessageView> = remaining.iter().take(limit as usize).cloned().collect();
            let next = if remaining.len() > page.len() {
                page.last().map(|message| message.sequence)
            } else {
                None
            };
            Ok(HistoryResponse {
                messages: page,
                next,
            })
        }

        fn send_message(&mut self, room: &str, body: SendMessageBody) -> Result<MessageView> {
            self.calls.push(format!("send:{room}"));
            self.sent.push((room.to_owned(), body));
            self.sends
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted send answer")))
        }

        fn relay_events(
            &mut self,
            session: &str,
            query: GetV1SessionsSessionRelayEventsQuery,
        ) -> Result<RelayPage> {
            let after = query.after.unwrap_or(0);
            let limit = self
                .relay_page_limit
                .map(|cap| cap as u64)
                .unwrap_or_else(|| query.limit.unwrap_or(RELAY_PAGE_SIZE))
                .max(1);
            self.calls.push(format!("relay:{session}:{after}:{limit}"));
            let all = self.relay.get(session).cloned().unwrap_or_default();
            let mut remaining: Vec<RelayEvent> = all
                .into_iter()
                .filter(|event| event.sequence.max(0) as u64 > after)
                .collect();
            remaining.sort_by_key(|left| left.sequence);
            let page: Vec<RelayEvent> = remaining.iter().take(limit as usize).cloned().collect();
            let next = if remaining.len() > page.len() {
                page.last().map(|event| event.sequence)
            } else {
                None
            };
            Ok(RelayPage { events: page, next })
        }

        fn relay_state(&mut self, session: &str) -> Result<RelayState> {
            self.calls.push(format!("relay-state:{session}"));
            Ok(self
                .relay_states
                .get(session)
                .cloned()
                .unwrap_or_else(|| relay_state(session, true)))
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::testing::{ScriptedRooms, member, message, relay_event, relay_state, room};
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    fn type_text(state: &mut RoomsState, text: &str) {
        for character in text.chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
    }

    fn numbered(count: i64) -> Vec<MessageView> {
        (1..=count)
            .map(|sequence| {
                message(
                    sequence,
                    &format!("srv-{sequence}"),
                    "alice",
                    &format!("body {sequence}"),
                )
            })
            .collect()
    }

    fn opened(messages: Vec<MessageView>) -> (RoomsState, ScriptedRooms, Instant) {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.queue_rooms(vec![room("room_1", "General")]);
        client.set_history("room_1", messages);
        let mut state = RoomsState::new();
        state.set_member(Some("ops@example.com".to_owned()));
        state.drive(&mut client, base);
        state.open_room("room_1");
        state.drive(&mut client, base);
        (state, client, base)
    }

    #[test]
    fn merge_keeps_the_server_id_and_deduplicates_a_repeated_page() {
        let existing = vec![
            message(1, "srv-1", "alice", "one"),
            message(2, "srv-2", "bob", "two"),
        ];
        let incoming = vec![
            message(2, "srv-2", "bob", "two, edited"),
            message(3, "srv-3", "carol", "three"),
        ];
        let merged = merge_messages(&existing, &incoming);
        let ids: Vec<&str> = merged.iter().map(|message| message.id.as_str()).collect();
        assert_eq!(ids, ["srv-1", "srv-2", "srv-3"]);
        assert_eq!(merged[1].body, "two, edited");
        assert_eq!(head_cursor(&merged), Some(3));
        assert_eq!(tail_cursor(&merged), Some(1));
    }

    #[test]
    fn ordering_is_by_sequence_then_id_not_by_timestamp() {
        let mut first = message(2, "srv-b", "alice", "two");
        first.created_at = "2026-10-29T00:00:09Z".to_owned();
        let mut second = message(2, "srv-a", "alice", "two again");
        second.created_at = "2026-10-29T00:00:01Z".to_owned();
        let merged = merge_messages(&[], &[first, second]);
        let ids: Vec<&str> = merged.iter().map(|message| message.id.as_str()).collect();
        assert_eq!(ids, ["srv-a", "srv-b"]);
    }

    #[test]
    fn opening_a_room_keeps_the_newest_window_and_marks_the_rest_older() {
        let (state, _client, _base) = opened(numbered(260));
        assert_eq!(state.messages().len(), ROOM_WINDOW);
        assert_eq!(state.messages()[0].id, "srv-61");
        assert_eq!(
            state.messages().last().map(|message| message.id.as_str()),
            Some("srv-260")
        );
        assert!(state.has_older());
        assert_eq!(state.open_room_id(), Some("room_1"));
    }

    #[test]
    fn scrolling_up_loads_older_pages_without_duplicating_messages() {
        let (mut state, mut client, base) = opened(numbered(260));

        // Scroll to the top of the transcript; the next drive reads one older
        // page through the forward cursor.
        for _ in 0..state.transcript_lines() + 2 {
            state.on_key(key(KeyCode::Up));
        }
        assert!(state.has_older());
        state.drive(&mut client, base + Duration::from_millis(10));

        let mut ids: Vec<&str> = state
            .messages()
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        assert_eq!(ids.first(), Some(&"srv-11"));
        assert_eq!(ids.len(), 250);
        assert_eq!(ids.len(), {
            let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
            unique.len()
        });
        assert!(state.has_older());

        // A second walk reaches the room's start and stops asking.
        for _ in 0..state.transcript_lines() + 2 {
            state.on_key(key(KeyCode::Up));
        }
        state.drive(&mut client, base + Duration::from_millis(20));
        ids = state
            .messages()
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        assert_eq!(ids.first(), Some(&"srv-1"));
        assert_eq!(ids.len(), 260);
        assert!(!state.has_older());

        let calls_before = client.calls.len();
        for _ in 0..10 {
            state.on_key(key(KeyCode::Up));
        }
        state.drive(&mut client, base + Duration::from_millis(30));
        assert_eq!(
            client.calls.len(),
            calls_before,
            "a finished walk asks for no older page"
        );
        state.drive(&mut client, base + LIVE_INTERVAL);
        assert!(
            client.calls[calls_before..]
                .iter()
                .any(|call| call == "history:room_1:260:50"),
            "the live poll reads after the head instead: {:?}",
            &client.calls[calls_before..]
        );
    }

    #[test]
    fn older_messages_page_returns_the_newest_page_below_the_cursor() {
        let mut client = ScriptedRooms::new();
        client.set_history("room_1", numbered(120));
        let page = older_messages_page(&mut client, "room_1", 101, 50).expect("a page");
        let ids: Vec<&str> = page
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        assert_eq!(ids.first(), Some(&"srv-51"));
        assert_eq!(ids.last(), Some(&"srv-100"));
        assert!(page.has_older);
        assert_eq!(
            client.calls,
            ["history:room_1:51:50", "history:room_1:2:50"],
            "the walk steps the forward cursor back one page at a time"
        );
    }

    #[test]
    fn older_messages_page_stops_at_the_start_without_gaps() {
        let mut client = ScriptedRooms::new();
        // Sequences 5, 10 and 15: the room's first sequence is not 1.
        let gapped = [5_i64, 10, 15]
            .into_iter()
            .map(|sequence| {
                message(
                    sequence,
                    &format!("srv-{sequence}"),
                    "alice",
                    &format!("body {sequence}"),
                )
            })
            .collect();
        client.set_history("room_gaps", gapped);
        let page = older_messages_page(&mut client, "room_gaps", 20, 10).expect("a page");
        assert_eq!(page.messages.len(), 3);
        assert!(!page.has_older);
    }

    #[test]
    fn a_live_message_appends_and_keeps_its_server_id() {
        let (mut state, mut client, base) = opened(numbered(3));
        client.push_message(
            "room_1",
            message(4, "srv-live-4", "bob", "arrived while open"),
        );
        state.drive(&mut client, base + LIVE_INTERVAL);
        assert_eq!(
            state.messages().last().map(|message| message.id.as_str()),
            Some("srv-live-4")
        );
        assert_eq!(
            state.messages().last().map(|message| message.sequence),
            Some(4)
        );

        // A repeated poll uses the head as its cursor, so the same message is
        // never appended twice.
        state.drive(&mut client, base + LIVE_INTERVAL * 2);
        let ids: Vec<&str> = state
            .messages()
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        assert_eq!(ids, ["srv-1", "srv-2", "srv-3", "srv-live-4"]);
    }

    #[test]
    fn composing_keeps_newlines_and_sends_only_on_ctrl_s() {
        let (mut state, mut client, base) = opened(numbered(2));
        client.queue_send(message(3, "srv-sent-3", "ops@example.com", "first\nsecond"));

        state.on_key(key(KeyCode::Char('c')));
        assert!(state.composer().is_some());
        type_text(&mut state, "first");
        state.on_key(key(KeyCode::Enter));
        type_text(&mut state, "second");

        let composer = state.composer().expect("the composer stays open");
        assert_eq!(composer.body(), "first\nsecond");
        assert!(
            !client.calls.iter().any(|call| call.starts_with("send:")),
            "Enter inserts a newline and never sends"
        );

        state.on_key(ctrl('s'));
        state.drive(&mut client, base + Duration::from_millis(10));
        assert!(state.composer().is_none(), "Ctrl-S sent the message");
        assert_eq!(client.sent.len(), 1);
        assert_eq!(client.sent[0].0, "room_1");
        assert_eq!(client.sent[0].1.body, "first\nsecond");
        assert_eq!(
            state.messages().last().map(|message| message.id.as_str()),
            Some("srv-sent-3"),
            "the pool's stored view is merged with its server id"
        );
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("srv-sent-3")),
            "{:?}",
            state.notice()
        );
    }

    #[test]
    fn a_blank_composer_sends_nothing_and_a_failed_send_keeps_the_text() {
        let (mut state, mut client, base) = opened(numbered(1));
        state.on_key(key(KeyCode::Char('c')));
        state.on_key(ctrl('s'));
        state.drive(&mut client, base + Duration::from_millis(10));
        assert!(client.sent.is_empty());
        assert!(state.composer().is_some());
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("Write a message")),
            "{:?}",
            state.notice()
        );

        type_text(&mut state, "will fail");
        state.on_key(ctrl('s'));
        state.drive(&mut client, base + Duration::from_millis(20));
        assert!(state.composer().is_some(), "the text survives the failure");
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("send failed")),
            "{:?}",
            state.notice()
        );
    }

    #[test]
    fn composing_owns_its_keys_so_q_does_not_quit_the_console() {
        let (mut state, _client, _base) = opened(numbered(1));
        state.on_key(key(KeyCode::Char('c')));
        assert!(state.on_key(key(KeyCode::Char('q'))));
        assert_eq!(state.composer().map(Composer::body), Some("q"));
        state.on_key(key(KeyCode::Esc));
        assert!(state.composer().is_none());
    }

    #[test]
    fn opening_a_room_reads_its_members_as_presence() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.queue_rooms(vec![room("room_1", "General")]);
        client.set_history("room_1", numbered(1));
        client.queue_members(vec![
            member("alice", "owner", "user"),
            member("swaygent", "member", "session"),
        ]);
        let mut state = RoomsState::new();
        state.drive(&mut client, base);
        state.open_room("room_1");
        state.drive(&mut client, base);
        assert_eq!(state.members().len(), 2);
        assert_eq!(state.members()[0].role, "owner");

        client.queue_members(vec![member("alice", "owner", "user")]);
        state.drive(&mut client, base + PRESENCE_INTERVAL);
        assert_eq!(state.members().len(), 1, "presence refreshes on its clock");
    }

    #[test]
    fn the_room_list_refreshes_on_its_clock() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.queue_rooms(vec![room("room_1", "General")]);
        client.queue_rooms(vec![room("room_1", "General"), room("room_2", "Second")]);
        let mut state = RoomsState::new();
        state.drive(&mut client, base);
        assert_eq!(state.rooms().len(), 1);
        state.drive(&mut client, base + ROOMS_INTERVAL);
        assert_eq!(state.rooms().len(), 2);
        state.on_key(key(KeyCode::Down));
        assert_eq!(state.selected(), 1);
        state.on_key(key(KeyCode::Up));
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn enter_opens_the_highlighted_room_and_escape_closes_it() {
        let (mut state, _client, _base) = opened(numbered(2));
        state.close_room();
        assert!(state.open_room_id().is_none());
        state.on_key(key(KeyCode::Enter));
        assert_eq!(state.open_room_id(), Some("room_1"));
        state.on_key(key(KeyCode::Esc));
        assert!(state.open_room_id().is_none());
        assert!(state.messages().is_empty());
    }

    #[test]
    fn backfill_is_marked_until_the_cursor_reaches_the_head_then_live() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.relay_page_limit = Some(2);
        for sequence in 1..=3 {
            client.push_relay(
                "sess-1",
                relay_event(sequence, "output", serde_json::json!({"step": sequence})),
            );
        }
        client.set_relay_state(relay_state("sess-1", true));
        let mut state = RoomsState::new();
        state.follow_session("sess-1");

        state.drive(&mut client, base);
        let follow = state.relay().expect("a follow");
        assert_eq!(follow.phase(), FollowPhase::Backfilling);
        assert_eq!(follow.rows().len(), 2);
        assert!(
            follow
                .rows()
                .iter()
                .all(|row| row.origin == Origin::Backfill)
        );
        assert_eq!(follow.live_from(), None);
        assert_eq!(follow.publication().map(|state| state.enabled), Some(true));

        state.drive(&mut client, base);
        let follow = state.relay().expect("a follow");
        assert_eq!(follow.phase(), FollowPhase::Live);
        assert_eq!(follow.backfill_count(), 3);
        assert_eq!(follow.live_count(), 0);

        client.push_relay(
            "sess-1",
            relay_event(4, "status", serde_json::json!({"state": "done"})),
        );
        state.drive(&mut client, base + RELAY_POLL_INTERVAL);
        let follow = state.relay().expect("a follow");
        assert_eq!(follow.phase(), FollowPhase::Live);
        assert_eq!(follow.live_count(), 1);
        assert_eq!(follow.live_from(), Some(3));
        assert_eq!(
            follow.rows()[3].event.id,
            "evt-4",
            "the live event keeps its server id"
        );
        assert_eq!(follow.rows().last().map(|row| row.event.sequence), Some(4));
        assert_eq!(follow.cursor(), Some(4));

        let ids: Vec<&str> = follow
            .rows()
            .iter()
            .map(|row| row.event.id.as_str())
            .collect();
        assert_eq!(ids, ["evt-1", "evt-2", "evt-3", "evt-4"]);
    }

    #[test]
    fn an_empty_relay_joins_live_immediately() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        let mut state = RoomsState::new();
        state.follow_session("sess-empty");
        state.drive(&mut client, base);
        let follow = state.relay().expect("a follow");
        assert_eq!(follow.phase(), FollowPhase::Live);
        assert!(follow.rows().is_empty());
        assert_eq!(follow.live_from(), None);

        client.push_relay(
            "sess-empty",
            relay_event(1, "started", serde_json::json!({"host": "studio"})),
        );
        state.drive(&mut client, base + RELAY_POLL_INTERVAL);
        let follow = state.relay().expect("a follow");
        assert_eq!(follow.live_from(), Some(0));
        assert_eq!(follow.rows()[0].origin, Origin::Live);
    }

    #[test]
    fn the_follow_prompt_names_a_session_and_refuses_an_empty_one() {
        let mut state = RoomsState::new();
        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(state.follow_prompt(), Some(""));
        state.on_key(key(KeyCode::Enter));
        assert!(state.relay().is_none());
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("session id")),
            "{:?}",
            state.notice()
        );

        state.on_key(key(KeyCode::Char('f')));
        type_text(&mut state, "sess-9");
        state.on_key(key(KeyCode::Enter));
        assert_eq!(state.relay().map(RelayFollow::session), Some("sess-9"));
        assert_eq!(state.follow_prompt(), None);

        state.on_key(key(KeyCode::Char('x')));
        assert!(state.relay().is_none());
    }

    #[test]
    fn a_history_failure_is_a_problem_on_screen_not_a_panic() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.queue_rooms(vec![room("room_1", "General")]);
        client.history_problem = Some("connection refused".to_owned());
        let mut state = RoomsState::new();
        state.drive(&mut client, base);
        state.open_room("room_1");
        state.drive(&mut client, base);
        assert!(
            state
                .history_problem()
                .is_some_and(|problem| problem.contains("connection refused")),
            "{:?}",
            state.history_problem()
        );
        assert!(state.messages().is_empty());
    }

    #[test]
    fn an_empty_rooms_list_says_so_without_panicking() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        let mut state = RoomsState::new();
        state.drive(&mut client, base);
        assert!(state.rooms().is_empty());
        assert_eq!(state.selected_room(), None);
        state.on_key(key(KeyCode::Enter));
        assert!(state.open_room_id().is_none());
        state.on_key(key(KeyCode::Up));
        assert_eq!(state.selected(), 0);
    }
}
