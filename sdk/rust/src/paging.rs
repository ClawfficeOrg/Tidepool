//! Cursor paging helpers over the contract's page shapes.
//!
//! The hub returns `next` (a sequence cursor) for chat and relay history and
//! `nextCursor`/`hasMore` for activity. These iterators follow the cursor until
//! the server stops offering one, so callers never hand-roll the loop.
//!
//! They are part of the blocking facade, which is the one the console and the
//! reference host agent use. An async caller fetches one page at a time with the
//! generated request type; the single-page shape is already typed.

use std::collections::VecDeque;

use crate::blocking::Client;
use crate::generated::endpoints::{
    GetV1Activity, GetV1ActivityQuery, GetV1ChatroomsRoomMessages, GetV1ChatroomsRoomMessagesQuery,
    GetV1SessionsSessionRelayEvents, GetV1SessionsSessionRelayEventsQuery,
};
use crate::generated::types::{ActivityEntry, MessageView, RelayEvent};

/// How many pages to walk and where to start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageOptions {
    /// Sequence to resume after (chat and relay).
    pub after: Option<u64>,
    /// Page size the server should use.
    pub limit: Option<u64>,
    /// Stop after this many pages. `None` means follow the cursor to the end.
    pub max_pages: Option<usize>,
}

impl PageOptions {
    /// Start with the defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resume after a sequence.
    pub fn after(mut self, sequence: u64) -> Self {
        self.after = Some(sequence);
        self
    }

    /// Ask for a page size.
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Stop after this many pages.
    pub fn max_pages(mut self, pages: usize) -> Self {
        self.max_pages = Some(pages);
        self
    }
}

/// Iterate a room's messages in sequence order.
pub fn chat_messages<'a>(
    client: &'a Client,
    room: impl Into<String>,
    options: PageOptions,
) -> ChatMessages<'a> {
    ChatMessages {
        client,
        room: room.into(),
        after: options.after,
        limit: options.limit,
        max_pages: options.max_pages,
        pages: 0,
        done: false,
        buffered: VecDeque::new(),
    }
}

/// A room's messages, following `next` until the server stops offering one.
#[derive(Debug)]
pub struct ChatMessages<'a> {
    client: &'a Client,
    room: String,
    after: Option<u64>,
    limit: Option<u64>,
    max_pages: Option<usize>,
    pages: usize,
    done: bool,
    buffered: VecDeque<MessageView>,
}

impl Iterator for ChatMessages<'_> {
    type Item = crate::Result<MessageView>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(message) = self.buffered.pop_front() {
                return Some(Ok(message));
            }
            if self.done {
                return None;
            }
            let query = GetV1ChatroomsRoomMessagesQuery {
                after: self.after,
                limit: self.limit,
            };
            match self
                .client
                .call(&GetV1ChatroomsRoomMessages::new(self.room.clone()).with_query(query))
            {
                Ok(page) => {
                    self.pages += 1;
                    self.buffered.extend(page.messages);
                    self.after = page.next.and_then(|next| u64::try_from(next).ok());
                    if self.after.is_none() || self.reached_page_limit() {
                        self.done = true;
                    }
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            }
        }
    }
}

impl ChatMessages<'_> {
    fn reached_page_limit(&self) -> bool {
        self.max_pages.is_some_and(|max| self.pages >= max)
    }
}

/// Iterate a session's relayed events in sequence order.
pub fn relay_events<'a>(
    client: &'a Client,
    session: impl Into<String>,
    options: PageOptions,
) -> RelayEvents<'a> {
    RelayEvents {
        client,
        session: session.into(),
        after: options.after,
        limit: options.limit,
        max_pages: options.max_pages,
        pages: 0,
        done: false,
        buffered: VecDeque::new(),
    }
}

/// A session's relayed events, following `next` until there is no more.
#[derive(Debug)]
pub struct RelayEvents<'a> {
    client: &'a Client,
    session: String,
    after: Option<u64>,
    limit: Option<u64>,
    max_pages: Option<usize>,
    pages: usize,
    done: bool,
    buffered: VecDeque<RelayEvent>,
}

impl Iterator for RelayEvents<'_> {
    type Item = crate::Result<RelayEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.buffered.pop_front() {
                return Some(Ok(event));
            }
            if self.done {
                return None;
            }
            let query = GetV1SessionsSessionRelayEventsQuery {
                after: self.after,
                limit: self.limit,
            };
            match self
                .client
                .call(&GetV1SessionsSessionRelayEvents::new(self.session.clone()).with_query(query))
            {
                Ok(page) => {
                    self.pages += 1;
                    self.buffered.extend(page.events);
                    self.after = page.next.and_then(|next| u64::try_from(next).ok());
                    if self.after.is_none() || self.reached_page_limit() {
                        self.done = true;
                    }
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            }
        }
    }
}

impl RelayEvents<'_> {
    fn reached_page_limit(&self) -> bool {
        self.max_pages.is_some_and(|max| self.pages >= max)
    }
}

/// Iterate the activity feed, following `nextCursor` while `hasMore` is true.
pub fn activity<'a>(
    client: &'a Client,
    options: PageOptions,
    filters: ActivityFilters,
) -> Activity<'a> {
    Activity {
        client,
        cursor: None,
        limit: options.limit,
        max_pages: options.max_pages,
        filters,
        pages: 0,
        done: false,
        buffered: VecDeque::new(),
    }
}

/// Filters the activity feed accepts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityFilters {
    /// Project id.
    pub project: Option<String>,
    /// Provider name.
    pub provider: Option<String>,
    /// Trigger name.
    pub trigger: Option<String>,
    /// Run or entry status.
    pub status: Option<String>,
    /// Entry kind.
    pub kind: Option<String>,
}

/// The activity feed, following its cursor until `hasMore` is false.
#[derive(Debug)]
pub struct Activity<'a> {
    client: &'a Client,
    cursor: Option<String>,
    limit: Option<u64>,
    max_pages: Option<usize>,
    filters: ActivityFilters,
    pages: usize,
    done: bool,
    buffered: VecDeque<ActivityEntry>,
}

impl Iterator for Activity<'_> {
    type Item = crate::Result<ActivityEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.buffered.pop_front() {
                return Some(Ok(entry));
            }
            if self.done {
                return None;
            }
            let query = GetV1ActivityQuery {
                project: self.filters.project.clone(),
                provider: self.filters.provider.clone(),
                trigger: self.filters.trigger.clone(),
                status: self.filters.status.clone(),
                kind: self.filters.kind.clone(),
                cursor: self.cursor.clone(),
                limit: self.limit,
            };
            match self.client.call(&GetV1Activity::new().with_query(query)) {
                Ok(page) => {
                    self.pages += 1;
                    self.buffered.extend(page.items);
                    self.cursor = page.next_cursor;
                    let has_more = page.has_more && self.cursor.is_some();
                    if !has_more || self.reached_page_limit() {
                        self.done = true;
                    }
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            }
        }
    }
}

impl Activity<'_> {
    fn reached_page_limit(&self) -> bool {
        self.max_pages.is_some_and(|max| self.pages >= max)
    }
}
