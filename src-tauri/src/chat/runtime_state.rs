use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use super::agent::context_measure::LiveContextMeasurement;
use super::agent::SteeringMessage;
use super::types::ContextUsageSegment;

/// Process-local ownership for Chat run coordination.
///
/// The composition root deliberately exposes behavior, not these indexes.  In
/// particular, a conversation can own several generations/reply slots at once;
/// ending one run must never retire its siblings.
#[derive(Default)]
pub(crate) struct ChatRuntimeState {
    next_generation: AtomicU64,
    runs: Mutex<ChatRunIndexes>,
    /// Signalled whenever a conversation's last reply slot is retired, so a
    /// queued send can retry its atomic reservation instead of polling.
    reply_idle: tokio::sync::Notify,
    popout_create_lock: tokio::sync::Mutex<()>,
    conversation_create_lock: tokio::sync::Mutex<()>,
    /// Admitted IM turns. `cancel` sticks from stop until the turn finishes so a
    /// generation that has not been created yet cannot start. `generations` holds
    /// every model run this turn claimed; stop retires that whole set. Desktop
    /// sends never set `active`, and a generation started while no IM turn is
    /// admitted is not part of the lineage.
    im_turns: parking_lot::Mutex<ImTurnFlags>,
}

#[derive(Default)]
struct ImTurnFlags {
    active: HashSet<String>,
    cancel: HashSet<String>,
    generations: HashMap<String, HashSet<u64>>,
    children: HashMap<String, HashSet<String>>,
}

/// Whether a child execution belongs to a generation this IM turn claimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImChild {
    Inherited,
    Stopped,
    NotIm,
}

#[derive(Default)]
struct ChatRunIndexes {
    active_generations: HashMap<String, HashSet<u64>>,
    active_replies: HashMap<String, HashSet<String>>,
    pending_steering: HashMap<String, Vec<SteeringMessage>>,
    pending_follow_up: HashMap<String, Vec<SteeringMessage>>,
    pending_goal_user_queue: HashSet<String>,
    auto_compact_failures: HashMap<String, u32>,
    context_measurements: HashMap<String, LiveContextMeasurement>,
}

impl ChatRuntimeState {
    pub(crate) fn begin_generation(&self, conversation_id: &str) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.indexes()
            .active_generations
            .entry(conversation_id.to_string())
            .or_default()
            .insert(generation);
        generation
    }

    /// Starts a generation unless this admitted IM turn was already stopped.
    /// The cancel flag and the lineage insert share one critical section, so a
    /// stop that lands first cannot lose to a later insert, and an insert that
    /// lands first stays in the set stop retires. A conversation with no admitted
    /// IM turn still receives a generation, and that id is not recorded here.
    pub(crate) fn begin_generation_unless_im_stopped(&self, conversation_id: &str) -> Option<u64> {
        let mut turns = self.im_turns.lock();
        let admitted = turns.active.contains(conversation_id);
        if admitted && turns.cancel.contains(conversation_id) {
            return None;
        }
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.runs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .active_generations
            .entry(conversation_id.to_string())
            .or_default()
            .insert(generation);
        if admitted {
            turns
                .generations
                .entry(conversation_id.to_string())
                .or_default()
                .insert(generation);
        }
        Some(generation)
    }

    pub(crate) fn cancel_conversation(&self, conversation_id: &str) {
        let mut indexes = self.indexes();
        indexes.active_generations.remove(conversation_id);
        indexes.pending_steering.remove(conversation_id);
        indexes.pending_follow_up.remove(conversation_id);
    }

    pub(crate) fn end_generation(&self, conversation_id: &str, generation: u64) {
        let mut indexes = self.indexes();
        retire_generation(&mut indexes, conversation_id, generation);
    }

    /// Reply slot and generation are retired as one operation. Releasing the
    /// reply slot first would let a new send begin between two independent
    /// locks, leaving old pending input attached to the new run.
    pub(crate) fn finish_reply_generation(
        &self,
        conversation_id: &str,
        run_id: &str,
        generation: u64,
    ) {
        let idle = {
            let mut indexes = self.indexes();
            retire_generation(&mut indexes, conversation_id, generation);
            retire_reply(&mut indexes, conversation_id, run_id)
        };
        if idle {
            self.reply_idle.notify_waiters();
        }
    }

    pub(crate) fn is_generation_active(&self, conversation_id: &str, generation: u64) -> bool {
        self.indexes()
            .active_generations
            .get(conversation_id)
            .is_some_and(|active| active.contains(&generation))
    }

    pub(crate) fn has_active_generation(&self, conversation_id: &str) -> bool {
        self.indexes()
            .active_generations
            .get(conversation_id)
            .is_some_and(|active| !active.is_empty())
    }

    pub(crate) fn push_steering(&self, conversation_id: &str, message: SteeringMessage) -> bool {
        let mut indexes = self.indexes();
        if !has_active_generation(&indexes, conversation_id) {
            return false;
        }
        indexes
            .pending_steering
            .entry(conversation_id.to_string())
            .or_default()
            .push(message);
        true
    }

    pub(crate) fn take_steering(&self, conversation_id: &str) -> Vec<SteeringMessage> {
        self.indexes()
            .pending_steering
            .remove(conversation_id)
            .unwrap_or_default()
    }

    pub(crate) fn push_follow_up(&self, conversation_id: &str, message: SteeringMessage) -> bool {
        let mut indexes = self.indexes();
        if !has_active_generation(&indexes, conversation_id) {
            return false;
        }
        indexes
            .pending_follow_up
            .entry(conversation_id.to_string())
            .or_default()
            .push(message);
        true
    }

    pub(crate) fn take_follow_up(&self, conversation_id: &str) -> Vec<SteeringMessage> {
        self.indexes()
            .pending_follow_up
            .remove(conversation_id)
            .unwrap_or_default()
    }

    pub(crate) fn has_pending_input(&self, conversation_id: &str) -> bool {
        let indexes = self.indexes();
        indexes
            .pending_steering
            .get(conversation_id)
            .is_some_and(|messages| !messages.is_empty())
            || indexes
                .pending_follow_up
                .get(conversation_id)
                .is_some_and(|messages| !messages.is_empty())
    }

    pub(crate) fn set_goal_user_queue_pending(&self, conversation_id: &str, pending: bool) {
        let mut indexes = self.indexes();
        if pending {
            indexes
                .pending_goal_user_queue
                .insert(conversation_id.to_string());
        } else {
            indexes.pending_goal_user_queue.remove(conversation_id);
        }
    }

    pub(crate) fn has_goal_user_queue_pending(&self, conversation_id: &str) -> bool {
        self.indexes()
            .pending_goal_user_queue
            .contains(conversation_id)
    }

    pub(crate) fn begin_im_turn(&self, conversation_id: &str) {
        self.im_turns
            .lock()
            .active
            .insert(conversation_id.to_string());
    }

    pub(crate) fn end_im_turn(&self, conversation_id: &str) {
        let mut turns = self.im_turns.lock();
        turns.active.remove(conversation_id);
        turns.cancel.remove(conversation_id);
        turns.generations.remove(conversation_id);
        turns.children.remove(conversation_id);
    }

    pub(crate) fn im_turn_active(&self, conversation_id: &str) -> bool {
        self.im_turns.lock().active.contains(conversation_id)
    }

    pub(crate) fn im_turn_cancel_latched(&self, conversation_id: &str) -> bool {
        self.im_turns.lock().cancel.contains(conversation_id)
    }

    /// Records stop for an admitted IM turn and retires every generation this
    /// turn claimed. A desktop generation on the same conversation stays
    /// active. `None` means this conversation is not in an IM turn, so the
    /// caller must not cancel anything. The vec is every child execution that
    /// inherited one of those generations; desktop children are never included.
    /// Claimed generations stay recorded until the turn ends, so a child that
    /// registers after stop is refused instead of starting.
    pub(crate) fn cancel_im_lineage(&self, conversation_id: &str) -> Option<Vec<String>> {
        let mut turns = self.im_turns.lock();
        if !turns.active.contains(conversation_id) {
            return None;
        }
        turns.cancel.insert(conversation_id.to_string());
        let generations = turns
            .generations
            .get(conversation_id)
            .cloned()
            .unwrap_or_default();
        let children = turns
            .children
            .remove(conversation_id)
            .map(|runs| runs.into_iter().collect())
            .unwrap_or_default();
        drop(turns);
        if !generations.is_empty() {
            let mut indexes = self.runs.lock().unwrap_or_else(|error| error.into_inner());
            for generation in generations {
                retire_generation(&mut indexes, conversation_id, generation);
            }
        }
        Some(children)
    }

    /// Tags a child execution when its parent generation is one this IM turn
    /// claimed. A desktop generation does not match, so the child is not part
    /// of the lineage. `Stopped` means that IM generation was already cancelled
    /// and this child must not start.
    pub(crate) fn note_im_child(
        &self,
        conversation_id: &str,
        parent_generation: u64,
        child_run: &str,
    ) -> ImChild {
        let mut turns = self.im_turns.lock();
        let claimed = turns
            .generations
            .get(conversation_id)
            .is_some_and(|generations| generations.contains(&parent_generation));
        if !claimed {
            return ImChild::NotIm;
        }
        if turns.cancel.contains(conversation_id) {
            return ImChild::Stopped;
        }
        turns
            .children
            .entry(conversation_id.to_string())
            .or_default()
            .insert(child_run.to_string());
        ImChild::Inherited
    }

    pub(crate) fn im_generation_stopped(
        &self,
        conversation_id: &str,
        parent_generation: u64,
    ) -> bool {
        let turns = self.im_turns.lock();
        turns.cancel.contains(conversation_id)
            && turns
                .generations
                .get(conversation_id)
                .is_some_and(|generations| generations.contains(&parent_generation))
    }

    pub(crate) fn forget_conversation(&self, conversation_id: &str) {
        self.end_im_turn(conversation_id);
        {
            let mut indexes = self.indexes();
            indexes.active_generations.remove(conversation_id);
            indexes.active_replies.remove(conversation_id);
            indexes.pending_steering.remove(conversation_id);
            indexes.pending_follow_up.remove(conversation_id);
            indexes.pending_goal_user_queue.remove(conversation_id);
            indexes.auto_compact_failures.remove(conversation_id);
            indexes.context_measurements.remove(conversation_id);
        }
        self.reply_idle.notify_waiters();
    }

    pub(crate) fn context_measurement(
        &self,
        conversation_id: &str,
    ) -> Option<LiveContextMeasurement> {
        self.indexes()
            .context_measurements
            .get(conversation_id)
            .cloned()
    }

    /// Raise the stored floor to the persisted snapshot. Never lowers a newer in-memory report.
    /// An empty slot copies the disk measurement so a refresh cannot hide a valid report.
    pub(crate) fn seed_context_measurement(
        &self,
        conversation_id: &str,
        lifecycle_id: u64,
        seq: u64,
        stored: Option<&crate::chat::types::ContextRequestMeasurement>,
    ) {
        let mut indexes = self.indexes();
        let slot = indexes
            .context_measurements
            .entry(conversation_id.to_string())
            .or_default();
        if lifecycle_id > slot.lifecycle_id {
            *slot = Default::default();
            slot.lifecycle_id = lifecycle_id;
        }
        if slot.request_id.is_empty() && slot.seq <= seq {
            slot.seq = seq;
            slot.lifecycle_id = lifecycle_id;
            if let Some(stored) =
                stored.filter(|stored| stored.lifecycle_id == lifecycle_id && stored.seq == seq)
            {
                slot.provider_id = stored.provider_id.clone();
                slot.model = stored.model.clone();
                slot.reported_tokens = stored.reported_tokens;
                slot.segments = stored.segments.clone();
                slot.categories_published = true;
            }
        }
    }

    pub(crate) fn bind_prepared_context(
        &self,
        conversation_id: &str,
        request_id: &str,
        message_id: &str,
        provider_id: &str,
        model: &str,
        segments: &[ContextUsageSegment],
        run_cache: Option<(u64, u64)>,
    ) -> LiveContextMeasurement {
        let mut indexes = self.indexes();
        let slot = indexes
            .context_measurements
            .entry(conversation_id.to_string())
            .or_default();
        let previous = slot.stored();
        slot.last_reported = (previous.reported_tokens.is_some()
            && previous.provider_id == provider_id
            && previous.model == model)
            .then_some(previous);
        slot.seq = slot.seq.saturating_add(1);
        slot.request_id = request_id.to_string();
        slot.message_id = message_id.to_string();
        slot.provider_id = provider_id.to_string();
        slot.model = model.to_string();
        slot.segments = segments.to_vec();
        // Never attach an earlier request's report to newly measured material.
        slot.reported_tokens = None;
        slot.categories_published = false;
        slot.report_received = false;
        slot.run_cache = run_cache;
        slot.clone()
    }

    /// Apply a provider report only to the request currently bound. A late report
    /// after invalidation or a newer bind is ignored. Categories ride the first
    /// report of the request; later deltas only move the token count.
    pub(crate) fn report_context_tokens(
        &self,
        conversation_id: &str,
        run_id: &str,
        tokens: u64,
    ) -> Option<(LiveContextMeasurement, bool)> {
        let mut indexes = self.indexes();
        let slot = indexes.context_measurements.get_mut(conversation_id)?;
        if slot.request_id != run_id || run_id.is_empty() {
            return None;
        }
        slot.seq = slot.seq.saturating_add(1);
        slot.reported_tokens = Some(tokens);
        slot.last_reported = None;
        slot.report_received = true;
        let include_segments = !slot.categories_published;
        slot.categories_published = true;
        Some((slot.clone(), include_segments))
    }

    /// Publish unknown only if this lifecycle has never received a valid report.
    /// A request without usage otherwise leaves the last coherent report visible.
    pub(crate) fn finish_unreported_context(
        &self,
        conversation_id: &str,
        run_id: &str,
    ) -> Option<LiveContextMeasurement> {
        let mut indexes = self.indexes();
        let slot = indexes.context_measurements.get_mut(conversation_id)?;
        if slot.request_id != run_id
            || run_id.is_empty()
            || slot.report_received
            || slot.stored().reported_tokens.is_some()
        {
            return None;
        }
        slot.seq = slot.seq.saturating_add(1);
        slot.reported_tokens = None;
        Some(slot.clone())
    }

    pub(crate) fn note_context_run_cache(
        &self,
        conversation_id: &str,
        run_id: &str,
        run_cache: Option<(u64, u64)>,
    ) {
        let mut indexes = self.indexes();
        if let Some(slot) = indexes.context_measurements.get_mut(conversation_id) {
            if !run_id.is_empty() && slot.request_id == run_id {
                slot.run_cache = run_cache;
            }
        }
    }

    pub(crate) fn invalidate_context_display(
        &self,
        conversation_id: &str,
    ) -> LiveContextMeasurement {
        let mut indexes = self.indexes();
        let slot = indexes
            .context_measurements
            .entry(conversation_id.to_string())
            .or_default();
        slot.lifecycle_id = slot.lifecycle_id.saturating_add(1);
        slot.seq = slot.seq.saturating_add(1);
        slot.reported_tokens = None;
        slot.segments.clear();
        slot.last_reported = None;
        slot.request_id.clear();
        slot.message_id.clear();
        slot.run_cache = None;
        slot.clone()
    }

    /// Consecutive automatic compaction failures, kept for the life of the process like
    /// ZCode's per-session circuit breaker.
    pub(crate) fn auto_compact_failures(&self, conversation_id: &str) -> u32 {
        self.indexes()
            .auto_compact_failures
            .get(conversation_id)
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn set_auto_compact_failures(&self, conversation_id: &str, failures: u32) {
        let mut indexes = self.indexes();
        if failures == 0 {
            indexes.auto_compact_failures.remove(conversation_id);
        } else {
            indexes
                .auto_compact_failures
                .insert(conversation_id.to_string(), failures);
        }
    }

    pub(crate) fn try_begin_reply(&self, conversation_id: &str, run_id: &str) -> bool {
        let mut indexes = self.indexes();
        let runs = indexes
            .active_replies
            .entry(conversation_id.to_string())
            .or_default();
        runs.insert(run_id.to_string())
    }

    pub(crate) fn try_reserve_send(&self, conversation_id: &str, run_id: &str) -> bool {
        let mut indexes = self.indexes();
        let runs = indexes
            .active_replies
            .entry(conversation_id.to_string())
            .or_default();
        if !runs.is_empty() {
            return false;
        }
        runs.insert(run_id.to_string());
        true
    }

    pub(crate) fn has_any_active_reply(&self) -> bool {
        self.indexes()
            .active_replies
            .values()
            .any(|runs| !runs.is_empty())
    }

    pub(crate) fn has_active_reply(&self, conversation_id: &str) -> bool {
        self.indexes()
            .active_replies
            .get(conversation_id)
            .is_some_and(|runs| !runs.is_empty())
    }

    pub(crate) fn end_reply(&self, conversation_id: &str, run_id: &str) {
        let idle = {
            let mut indexes = self.indexes();
            retire_reply(&mut indexes, conversation_id, run_id)
        };
        if idle {
            self.reply_idle.notify_waiters();
        }
    }

    /// Waits until the conversation has no reply at all, then takes the same
    /// atomic send reservation as `try_reserve_send`. Waiters re-check after
    /// every idle signal, so a user send that wins the race simply queues this
    /// one behind it again.
    pub(crate) async fn reserve_send_when_idle(&self, conversation_id: &str, run_id: &str) {
        loop {
            let notified = self.reply_idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.try_reserve_send(conversation_id, run_id) {
                return;
            }
            notified.await;
        }
    }

    pub(crate) async fn lock_popout_creation(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.popout_create_lock.lock().await
    }

    pub(crate) async fn lock_conversation_creation(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.conversation_create_lock.lock().await
    }

    fn indexes(&self) -> std::sync::MutexGuard<'_, ChatRunIndexes> {
        self.runs.lock().unwrap_or_else(|error| error.into_inner())
    }
}

fn has_active_generation(indexes: &ChatRunIndexes, conversation_id: &str) -> bool {
    indexes
        .active_generations
        .get(conversation_id)
        .is_some_and(|active| !active.is_empty())
}

fn retire_generation(indexes: &mut ChatRunIndexes, conversation_id: &str, generation: u64) {
    if let Some(active) = indexes.active_generations.get_mut(conversation_id) {
        active.remove(&generation);
        if active.is_empty() {
            indexes.active_generations.remove(conversation_id);
            indexes.pending_steering.remove(conversation_id);
            indexes.pending_follow_up.remove(conversation_id);
        }
    }
}

/// IM turns approve tools without writing session consent. Desktop keeps its own grant.
pub(crate) fn im_turn_allows_tools(im_turn_active: bool) -> bool {
    im_turn_active
}

pub(crate) fn im_session_consent(im_turn_active: bool, desktop_session_consent: bool) -> bool {
    im_turn_active || desktop_session_consent
}

/// Returns true when this retired the conversation's last reply slot.
fn retire_reply(indexes: &mut ChatRunIndexes, conversation_id: &str, run_id: &str) -> bool {
    if let Some(runs) = indexes.active_replies.get_mut(conversation_id) {
        if !runs.remove(run_id) {
            return false;
        }
        if runs.is_empty() {
            indexes.active_replies.remove(conversation_id);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_runs_end_independently_and_cancel_is_conversation_scoped() {
        let runtime = ChatRuntimeState::default();
        let first = runtime.begin_generation("a");
        let sibling = runtime.begin_generation("a");
        let other = runtime.begin_generation("b");

        runtime.end_generation("a", first);
        assert!(!runtime.is_generation_active("a", first));
        assert!(runtime.is_generation_active("a", sibling));
        assert!(runtime.is_generation_active("b", other));

        runtime.cancel_conversation("a");
        assert!(!runtime.is_generation_active("a", sibling));
        assert!(runtime.is_generation_active("b", other));
    }

    #[test]
    fn im_turn_scope_does_not_cover_another_conversation() {
        let runtime = ChatRuntimeState::default();
        runtime.begin_im_turn("conv_im");
        assert!(runtime.im_turn_active("conv_im"));
        assert!(!runtime.im_turn_active("conv_desktop"));
        assert!(im_turn_allows_tools(runtime.im_turn_active("conv_im")));
        assert!(!im_turn_allows_tools(
            runtime.im_turn_active("conv_desktop")
        ));
        assert_eq!(
            im_session_consent(true, false),
            true,
            "an IM turn allows tools without a stored desktop grant"
        );
        assert_eq!(im_session_consent(false, false), false);
        assert_eq!(im_session_consent(false, true), true);
        runtime.end_im_turn("conv_im");
        assert!(!runtime.im_turn_active("conv_im"));
        assert!(!im_turn_allows_tools(false));
    }

    #[test]
    fn stop_before_generation_admits_nothing_and_spares_a_later_desktop_run() {
        let runtime = ChatRuntimeState::default();
        runtime.begin_im_turn("conv");
        let started = runtime
            .begin_generation_unless_im_stopped("conv")
            .expect("an IM turn that has not been stopped can generate");
        let desktop = runtime.begin_generation("conv");
        assert_eq!(
            runtime.note_im_child("conv", desktop, "desktop-child"),
            ImChild::NotIm,
            "a desktop child does not inherit the IM generation"
        );
        assert_eq!(
            runtime.note_im_child("conv", started, "im-child"),
            ImChild::Inherited
        );
        let children = runtime
            .cancel_im_lineage("conv")
            .expect("an admitted turn can be stopped");
        assert_eq!(children, vec!["im-child".to_string()]);
        assert!(runtime.im_turn_cancel_latched("conv"));
        assert!(
            !runtime.is_generation_active("conv", started),
            "stop retires every generation this IM turn claimed"
        );
        assert!(
            runtime.is_generation_active("conv", desktop),
            "a desktop generation on the same conversation stays active"
        );
        assert_eq!(
            runtime.note_im_child("conv", started, "late-im-child"),
            ImChild::Stopped
        );
        assert!(
            runtime.begin_generation_unless_im_stopped("conv").is_none(),
            "stop before the next generation must not admit a model run"
        );
        assert!(
            runtime.is_generation_active("conv", desktop),
            "refusing the next IM generation must not retire the desktop run"
        );

        let other = runtime.begin_generation("other");
        runtime.end_im_turn("conv");
        assert!(!runtime.im_turn_cancel_latched("conv"));
        let desktop = runtime.begin_generation("conv");
        assert!(
            runtime.cancel_im_lineage("conv").is_none(),
            "stopping IM after the turn ended must not latch the desktop run"
        );
        assert!(runtime.is_generation_active("conv", desktop));
        assert!(runtime.is_generation_active("other", other));
        assert!(!im_turn_allows_tools(runtime.im_turn_active("conv")));
        assert!(!im_session_consent(runtime.im_turn_active("conv"), false));
    }

    #[test]
    fn im_stop_retires_every_admitted_generation_and_child() {
        let runtime = ChatRuntimeState::default();
        let desktop_reply = runtime
            .begin_generation_unless_im_stopped("conv")
            .expect("a reply with no admitted IM turn still starts");
        let desktop = runtime.begin_generation("conv");
        runtime.begin_im_turn("conv");
        runtime.begin_im_turn("other");
        let first = runtime
            .begin_generation_unless_im_stopped("conv")
            .expect("the first admitted arm starts");
        let second = runtime
            .begin_generation_unless_im_stopped("conv")
            .expect("a later admitted arm starts without replacing the first");
        let other = runtime
            .begin_generation_unless_im_stopped("other")
            .expect("another admitted IM turn starts on its own");
        assert_ne!(first, second);

        assert_eq!(
            runtime.note_im_child("conv", desktop_reply, "desktop-reply-child"),
            ImChild::NotIm
        );
        assert_eq!(
            runtime.note_im_child("conv", desktop, "desktop-child"),
            ImChild::NotIm
        );
        assert_eq!(
            runtime.note_im_child("conv", first, "arm-a-child"),
            ImChild::Inherited
        );
        assert_eq!(
            runtime.note_im_child("conv", second, "arm-b-child"),
            ImChild::Inherited
        );
        assert_eq!(
            runtime.note_im_child("other", other, "other-child"),
            ImChild::Inherited
        );

        let mut children = runtime
            .cancel_im_lineage("conv")
            .expect("an admitted turn can be stopped");
        children.sort();
        assert_eq!(
            children,
            vec!["arm-a-child".to_string(), "arm-b-child".to_string()]
        );
        assert!(runtime.im_turn_cancel_latched("conv"));
        assert!(!runtime.im_turn_cancel_latched("other"));
        for generation in [first, second] {
            assert!(!runtime.is_generation_active("conv", generation));
            assert!(runtime.im_generation_stopped("conv", generation));
            assert_eq!(
                runtime.note_im_child("conv", generation, "late-child"),
                ImChild::Stopped
            );
        }
        assert!(runtime.begin_generation_unless_im_stopped("conv").is_none());
        assert!(runtime.is_generation_active("conv", desktop_reply));
        assert!(runtime.is_generation_active("conv", desktop));
        assert!(runtime.is_generation_active("other", other));
        assert_eq!(
            runtime.note_im_child("other", other, "other-late"),
            ImChild::Inherited
        );
        assert!(
            runtime.cancel_im_lineage("conv").is_some(),
            "the latch stays until the turn ends"
        );
        assert!(runtime.is_generation_active("other", other));

        runtime.end_im_turn("conv");
        assert!(!runtime.im_turn_cancel_latched("conv"));
        assert!(!runtime.im_turn_active("conv"));
        runtime.begin_im_turn("conv");
        let next = runtime
            .begin_generation_unless_im_stopped("conv")
            .expect("ending the turn clears the cancel flag for the next round");
        assert!(runtime.is_generation_active("conv", next));
        assert_eq!(
            runtime.note_im_child("conv", next, "next-child"),
            ImChild::Inherited
        );
        assert!(runtime.is_generation_active("conv", desktop));
        assert!(runtime.is_generation_active("conv", desktop_reply));
        assert!(runtime.is_generation_active("other", other));
        assert!(!runtime.im_generation_stopped("conv", first));
    }

    #[test]
    fn im_stop_race_retires_or_refuses_each_registration() {
        let runtime = std::sync::Arc::new(ChatRuntimeState::default());
        runtime.begin_im_turn("conv");
        let desktop = runtime.begin_generation("conv");
        let other = runtime.begin_generation("other");
        let started = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let inherited = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stopper = {
            let runtime = std::sync::Arc::clone(&runtime);
            std::thread::spawn(move || runtime.cancel_im_lineage("conv"))
        };
        let mut arms = Vec::new();
        for index in 0..8 {
            let runtime = std::sync::Arc::clone(&runtime);
            let started = std::sync::Arc::clone(&started);
            let inherited = std::sync::Arc::clone(&inherited);
            arms.push(std::thread::spawn(move || {
                let Some(generation) = runtime.begin_generation_unless_im_stopped("conv") else {
                    return;
                };
                started
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(generation);
                let child = format!("arm-{index}");
                match runtime.note_im_child("conv", generation, &child) {
                    ImChild::Inherited => inherited
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(child),
                    ImChild::Stopped => {}
                    ImChild::NotIm => panic!("an admitted generation left the IM lineage"),
                }
            }));
        }
        let children = stopper
            .join()
            .expect("stop thread")
            .expect("the admitted turn is still open while stop runs");
        for arm in arms {
            arm.join().expect("arm thread");
        }

        let started = started.lock().unwrap_or_else(|error| error.into_inner());
        let inherited = inherited.lock().unwrap_or_else(|error| error.into_inner());
        for generation in started.iter() {
            assert!(!runtime.is_generation_active("conv", *generation));
            assert!(runtime.im_generation_stopped("conv", *generation));
            assert_eq!(
                runtime.note_im_child("conv", *generation, "after-stop"),
                ImChild::Stopped
            );
        }
        for child in inherited.iter() {
            assert!(
                children.iter().any(|stopped| stopped == child),
                "a child that inherited before stop must be in the cancelled set"
            );
        }
        assert!(runtime.begin_generation_unless_im_stopped("conv").is_none());
        assert!(runtime.is_generation_active("conv", desktop));
        assert!(runtime.is_generation_active("other", other));

        runtime.end_im_turn("conv");
        assert!(!runtime.im_turn_cancel_latched("conv"));
        runtime.begin_im_turn("conv");
        let next = runtime
            .begin_generation_unless_im_stopped("conv")
            .expect("the next round can generate after the stopped turn ends");
        assert!(runtime.is_generation_active("conv", next));
        assert!(runtime.is_generation_active("conv", desktop));
        assert!(runtime.is_generation_active("other", other));
    }

    #[test]
    fn reply_slots_are_per_run_but_send_reservation_is_per_conversation() {
        let runtime = ChatRuntimeState::default();
        assert!(runtime.try_begin_reply("a", "run-1"));
        assert!(runtime.try_begin_reply("a", "run-2"));
        assert!(!runtime.try_begin_reply("a", "run-1"));
        assert!(!runtime.try_reserve_send("a", "send"));

        runtime.end_reply("a", "run-1");
        assert!(runtime.has_active_reply("a"));
        runtime.end_reply("a", "run-2");
        assert!(runtime.try_reserve_send("a", "send"));
    }

    #[tokio::test]
    async fn queued_send_waits_for_every_reply_then_reserves() {
        let runtime = std::sync::Arc::new(ChatRuntimeState::default());
        assert!(runtime.try_begin_reply("a", "run-1"));
        assert!(runtime.try_begin_reply("a", "run-2"));

        let waiter = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.reserve_send_when_idle("a", "queued").await }
        });
        tokio::task::yield_now().await;
        runtime.end_reply("a", "run-1");
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished(), "one reply is still running");

        runtime.end_reply("a", "run-2");
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("idle signal must wake the queued send")
            .unwrap();
        assert!(
            !runtime.try_reserve_send("a", "user"),
            "queued send now owns the conversation"
        );
        runtime.end_reply("a", "queued");
        assert!(runtime.try_reserve_send("a", "user"));
    }

    #[test]
    fn forget_clears_only_one_conversation_runtime() {
        let runtime = ChatRuntimeState::default();
        let a = runtime.begin_generation("a");
        let b = runtime.begin_generation("b");
        assert!(runtime.try_begin_reply("a", "run-a"));
        runtime.set_goal_user_queue_pending("a", true);
        runtime.set_auto_compact_failures("a", 3);
        runtime.set_auto_compact_failures("b", 2);

        runtime.forget_conversation("a");

        assert!(!runtime.is_generation_active("a", a));
        assert!(!runtime.has_active_reply("a"));
        assert!(!runtime.has_goal_user_queue_pending("a"));
        assert_eq!(runtime.auto_compact_failures("a"), 0);
        assert!(runtime.is_generation_active("b", b));
        assert_eq!(runtime.auto_compact_failures("b"), 2);
        runtime.set_auto_compact_failures("b", 0);
        assert_eq!(runtime.auto_compact_failures("b"), 0);
    }

    #[test]
    fn cancelled_mailbox_input_cannot_leak_into_next_run() {
        let runtime = ChatRuntimeState::default();
        let old = runtime.begin_generation("conversation");
        assert!(runtime.push_steering(
            "conversation",
            SteeringMessage {
                id: "old".into(),
                text: "old".into(),
            },
        ));
        runtime.cancel_conversation("conversation");
        assert!(!runtime.is_generation_active("conversation", old));
        assert!(runtime.take_steering("conversation").is_empty());
        assert!(!runtime.push_steering(
            "conversation",
            SteeringMessage {
                id: "late".into(),
                text: "late".into(),
            },
        ));
        runtime.begin_generation("conversation");
        assert!(runtime.take_steering("conversation").is_empty());
    }

    #[test]
    fn last_run_finish_clears_pending_but_sibling_finish_preserves_it() {
        let runtime = ChatRuntimeState::default();
        let first = runtime.begin_generation("conversation");
        let sibling = runtime.begin_generation("conversation");
        assert!(runtime.push_follow_up(
            "conversation",
            SteeringMessage {
                id: "pending".into(),
                text: "continue".into(),
            },
        ));
        runtime.end_generation("conversation", first);
        assert!(runtime.has_pending_input("conversation"));
        runtime.end_generation("conversation", sibling);
        assert!(!runtime.has_pending_input("conversation"));
    }

    #[test]
    fn finishing_reply_retires_generation_and_slot_atomically() {
        let runtime = ChatRuntimeState::default();
        let generation = runtime.begin_generation("conversation");
        assert!(runtime.try_begin_reply("conversation", "run"));
        assert!(runtime.push_steering(
            "conversation",
            SteeringMessage {
                id: "old".into(),
                text: "old".into(),
            },
        ));

        runtime.finish_reply_generation("conversation", "run", generation);

        assert!(!runtime.is_generation_active("conversation", generation));
        assert!(!runtime.has_active_reply("conversation"));
        assert!(!runtime.has_pending_input("conversation"));
        assert!(runtime.try_reserve_send("conversation", "next"));
    }

    #[test]
    fn late_cache_report_cannot_overwrite_new_branch_request() {
        let runtime = ChatRuntimeState::default();
        runtime.bind_prepared_context(
            "conversation",
            "old",
            "first",
            "openai",
            "gpt-4o",
            &[],
            Some((100, 90)),
        );
        runtime.invalidate_context_display("conversation");
        runtime.bind_prepared_context(
            "conversation",
            "new",
            "second",
            "openai",
            "gpt-4o",
            &[],
            Some((100, 10)),
        );
        // This callback belongs to the old run, which has already lost ownership.
        runtime.note_context_run_cache("conversation", "old", Some((100, 90)));
        let (reported, _) = runtime
            .report_context_tokens("conversation", "new", 7_100)
            .unwrap();
        assert_eq!(reported.run_cache, Some((100, 10)));
        runtime.note_context_run_cache("conversation", "new", Some((200, 30)));
        let (reported, _) = runtime
            .report_context_tokens("conversation", "new", 8_000)
            .unwrap();
        assert_eq!(reported.run_cache, Some((200, 30)));
    }
}
