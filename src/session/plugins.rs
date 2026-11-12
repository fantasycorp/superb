//! The plugins of an open session: loading them, placing them in the
//! mixer, and keeping each instance across rebuilds.
//!
//! The memory holds a plugin insert as a reference, parameter values and
//! saved bytes (`Insert` with `Effect::Plugin`). This module holds what
//! cannot be saved: the loaded instance. An instance is made once, by the
//! session's [`PluginLoader`], and then lives in the running mixer. Each
//! rebuild puts a node in the new mixer that takes the instance over from
//! the old one at the swap (`PluginNode::adopting`), so its state and its
//! tails carry on. What the control side keeps is a [`LoadedPlugin`]: the
//! description, the parameters, the handle that reads values and saves
//! state while the instance processes, and the queue on which the instance
//! reports what it changed itself.

use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::primitives::plugins::{
    AudioPortLayout, ParamChange, ParamInfo, PluginChange, PluginControl, PluginDescriptor,
    PluginFormat, PluginInstance, PluginNode,
};
use crate::primitives::realtime::spsc::Consumer;
use crate::primitives::session::{commands, Effect, Insert, PluginRef};
use crate::primitives::{InsertId, Result, SuperbError};
use crate::session::{MixerHandles, Session};

/// Turns a plugin reference into a loaded instance. A session starts with
/// [`default_loader`]; replace [`Session::plugin_loader`] to host a format
/// of your own or to load from somewhere else.
pub type PluginLoader = Box<dyn FnMut(&PluginRef) -> Result<Box<dyn PluginInstance>> + Send>;

/// The loader a session starts with: CLAP bundles from their path (feature
/// `clap`). Other formats are refused.
pub fn default_loader() -> PluginLoader {
    Box::new(|plugin: &PluginRef| match plugin.format {
        #[cfg(feature = "clap")]
        PluginFormat::Clap => {
            let bundle = crate::primitives::plugins::clap::ClapBundle::load(&plugin.path)?;
            Ok(Box::new(bundle.instantiate(&plugin.id)?) as Box<dyn PluginInstance>)
        }
        #[cfg(not(feature = "clap"))]
        PluginFormat::Clap => Err(SuperbError::Unsupported(
            "CLAP hosting is disabled (feature `clap`)".into(),
        )),
        PluginFormat::Vst3 => Err(crate::primitives::plugins::vst3::unsupported()),
    })
}

/// A plugin insert as the session holds it: loaded, or the reason it is not.
#[derive(Debug)]
pub enum PluginSlot {
    /// The plugin is loaded and placed at every rebuild.
    Loaded(Box<LoadedPlugin>),
    /// The plugin could not be loaded. Its insert stays in the memory, is
    /// saved as it was, and is left out of the mixer; it is not tried again
    /// until the session is opened again.
    Missing(SuperbError),
}

/// The control side of one loaded plugin.
pub struct LoadedPlugin {
    /// What the plugin says it is.
    pub descriptor: PluginDescriptor,
    /// Its parameters, in the plugin's order.
    pub params: Vec<ParamInfo>,
    ports: AudioPortLayout,
    /// The instance's latency after it was last prepared.
    latency: Arc<AtomicUsize>,
    /// Reads values and saves state while the instance processes.
    control: Option<Box<dyn PluginControl>>,
    /// The instance, until a rebuild places it in the mixer.
    waiting: Option<Box<dyn PluginInstance>>,
    /// The parameter values the instance has been given, to tell what a
    /// rebuild after an undo has to send again.
    sent: Vec<(u32, f64)>,
    /// Whether the rebuild in progress has placed it.
    placed: bool,
    /// Where the instance reports what it changed itself; there from the
    /// first time it is placed, for as long as it lives.
    changes: Option<Consumer<PluginChange>>,
    /// What it has reported that `plugin::changes` has not handed out yet.
    heard: Vec<PluginChange>,
}

impl core::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("id", &self.descriptor.id)
            .field("params", &self.params.len())
            .field("in_mixer", &self.waiting.is_none())
            .finish()
    }
}

impl LoadedPlugin {
    /// The value a parameter has right now, read from the plugin.
    pub fn value(&self, param: u32) -> Result<f64> {
        match (&self.waiting, &self.control) {
            (Some(instance), _) => instance.param_value(param),
            (None, Some(control)) => control.param_value(param),
            (None, None) => Err(SuperbError::Unsupported(format!(
                "{} gives no way to read a parameter while it runs",
                self.descriptor.name
            ))),
        }
    }

    /// The plugin's state as bytes, taken now.
    fn state(&mut self) -> Result<Vec<u8>> {
        match (&mut self.waiting, &self.control) {
            (Some(instance), _) => instance.save_state(),
            (None, Some(control)) => control.save_state(),
            (None, None) => Err(SuperbError::Unsupported(format!(
                "{} gives no way to save its state while it runs",
                self.descriptor.name
            ))),
        }
    }
}

/// Loads the plugin an insert names and brings it to where the memory left
/// it: the saved bytes first, then the stored parameter values.
fn load(loader: &mut PluginLoader, insert: &Insert, channels: usize) -> Result<LoadedPlugin> {
    let Effect::Plugin(plugin) = &insert.effect else {
        return Err(SuperbError::InvalidArgument {
            what: "the insert is not a plugin",
        });
    };
    let mut instance = loader(plugin)?;
    let ports = instance.ports();
    if ports.inputs != channels || ports.outputs != channels {
        return Err(SuperbError::Unsupported(format!(
            "{} has {} input and {} output channels; an insert needs {channels} of each (instruments are not placed yet)",
            instance.descriptor().name,
            ports.inputs,
            ports.outputs
        )));
    }
    if !insert.preset.is_empty() {
        if let Err(error) = instance.load_state(&insert.preset) {
            log::warn!(
                "{}: its saved state was refused ({error}); it starts from its defaults",
                instance.descriptor().name
            );
        }
    }
    for (param, value) in &insert.state.params {
        let _ = instance.set_param(*param, *value);
    }
    Ok(LoadedPlugin {
        descriptor: instance.descriptor().clone(),
        params: instance.params().to_vec(),
        ports,
        latency: Arc::new(AtomicUsize::new(instance.latency_frames())),
        control: instance.control(),
        sent: insert.state.params.clone(),
        waiting: Some(instance),
        placed: false,
        changes: None,
        heard: Vec::new(),
    })
}

/// Makes sure the session holds a slot for a plugin insert, loading the
/// plugin if this is the first time it is asked for.
pub(crate) fn ensure(
    plugins: &mut Vec<(InsertId, PluginSlot)>,
    loader: &mut PluginLoader,
    insert: &Insert,
    channels: usize,
) -> usize {
    if let Some(at) = plugins.iter().position(|(id, _)| *id == insert.id()) {
        return at;
    }
    let slot = match load(loader, insert, channels) {
        Ok(loaded) => PluginSlot::Loaded(Box::new(loaded)),
        Err(error) => {
            log::warn!("a plugin insert is left out of the mixer: {error}");
            PluginSlot::Missing(error)
        }
    };
    plugins.push((insert.id(), slot));
    plugins.len() - 1
}

/// Loads a plugin for an insert that is about to be added, so a plugin
/// that cannot be loaded is refused there and then.
pub(crate) fn load_new(session: &mut Session, insert: &Insert) -> Result<()> {
    let loaded = load(&mut session.plugin_loader, insert, session.channels)?;
    session
        .plugins
        .push((insert.id(), PluginSlot::Loaded(Box::new(loaded))));
    Ok(())
}

/// The node for a plugin insert in the mixer being built: one that owns
/// the instance the first time, and afterwards one that takes the instance
/// over from the running mixer at the swap. `None` for a plugin that is
/// missing.
pub(crate) fn place(
    plugins: &mut Vec<(InsertId, PluginSlot)>,
    loader: &mut PluginLoader,
    insert: &Insert,
    channels: usize,
) -> Option<PluginNode> {
    let at = ensure(plugins, loader, insert, channels);
    let PluginSlot::Loaded(plugin) = &mut plugins[at].1 else {
        return None;
    };
    plugin.placed = true;
    let key = insert.id().raw();
    let node = match plugin.waiting.take() {
        Some(instance) => {
            let mut node = PluginNode::new(instance).carried_as(key);
            plugin.latency = node.latency_handle();
            plugin.changes = node.take_change_receiver();
            node
        }
        None => PluginNode::adopting(
            key,
            plugin.descriptor.name.clone(),
            plugin.ports,
            plugin.latency.load(Ordering::Relaxed),
        ),
    };
    node.bypass_handle()
        .store(insert.state.bypass, Ordering::Relaxed);
    Some(node)
}

/// The node for a plugin insert in a mixer built beside the running one (a
/// bounce): a second instance of the plugin, brought to the state the
/// running one has now. `None` for a plugin that is missing or cannot be
/// loaded twice.
pub(crate) fn place_copy(
    plugins: &mut Vec<(InsertId, PluginSlot)>,
    loader: &mut PluginLoader,
    insert: &Insert,
    channels: usize,
) -> Option<PluginNode> {
    let Effect::Plugin(plugin) = &insert.effect else {
        return None;
    };
    let at = ensure(plugins, loader, insert, channels);
    let PluginSlot::Loaded(running) = &mut plugins[at].1 else {
        return None;
    };
    let state = running.state().unwrap_or_else(|_| insert.preset.clone());
    let mut copy = match loader(plugin) {
        Ok(copy) => copy,
        Err(error) => {
            log::warn!(
                "{} is left out of the bounce: {error}",
                running.descriptor.name
            );
            return None;
        }
    };
    if !state.is_empty() {
        let _ = copy.load_state(&state);
    }
    for (param, value) in &insert.state.params {
        let _ = copy.set_param(*param, *value);
    }
    let node = PluginNode::new(copy);
    node.bypass_handle()
        .store(insert.state.bypass, Ordering::Relaxed);
    Some(node)
}

/// Before a build: nothing is placed yet.
pub(crate) fn begin_build(plugins: &mut [(InsertId, PluginSlot)]) {
    for (_, slot) in plugins {
        if let PluginSlot::Loaded(plugin) = slot {
            plugin.placed = false;
        }
    }
}

/// After a build, before the new mixer is published: sends each placed
/// plugin the parameter values the memory holds that it has not been given
/// (an undo took a value back, a redo brought one), and lets go of every
/// plugin the build did not place (its insert was removed, its track
/// archived), keeping its state in the memory first so it comes back as it
/// was.
pub(crate) fn finish_build(session: &mut Session, handles: &mut MixerHandles) {
    for (id, slot) in &mut session.plugins {
        let PluginSlot::Loaded(plugin) = slot else {
            continue;
        };
        let Ok(insert) = session.memory.insert(*id) else {
            continue;
        };
        if !plugin.placed {
            continue;
        }
        let wanted = &insert.state.params;
        let Some(live) = handles.plugin_mut(*id) else {
            continue;
        };
        for (param, value) in wanted {
            if !plugin.sent.contains(&(*param, *value)) {
                let _ = live.params.push(ParamChange::new(*param, *value));
            }
        }
        for (param, _) in &plugin.sent {
            if !wanted.iter().any(|(p, _)| p == param) {
                // The stored value is gone: back to the plugin's default.
                if let Some(info) = plugin.params.iter().find(|i| i.id == *param) {
                    let _ = live.params.push(ParamChange::new(*param, info.default));
                }
            }
        }
        plugin.sent = wanted.clone();
    }
    let mut at = 0;
    while at < session.plugins.len() {
        let (id, slot) = &mut session.plugins[at];
        let gone = match slot {
            PluginSlot::Loaded(plugin) if !plugin.placed => {
                if let (Ok(state), Ok(insert)) = (plugin.state(), session.memory.insert_mut(*id)) {
                    insert.preset = state;
                }
                true
            }
            PluginSlot::Loaded(_) => false,
            PluginSlot::Missing(_) => session.memory.insert(*id).is_err(),
        };
        if gone {
            session.plugins.remove(at);
        } else {
            at += 1;
        }
    }
}

/// Writes every loaded plugin's state into its insert in the memory, so
/// the memory can be saved, or an insert removed and brought back, with
/// the plugin as it is now. A plugin that cannot save keeps what the
/// memory had.
pub(crate) fn store_state(session: &mut Session, only: Option<InsertId>) {
    for (id, slot) in &mut session.plugins {
        if only.is_some_and(|only| only != *id) {
            continue;
        }
        let PluginSlot::Loaded(plugin) = slot else {
            continue;
        };
        if let (Ok(state), Ok(insert)) = (plugin.state(), session.memory.insert_mut(*id)) {
            insert.preset = state;
        }
    }
}

/// Takes in what every plugin has reported changing itself: each value
/// goes into the memory as an edit, as if `plugin::param` had set it (so
/// it is saved, and undo sends the old value back), and every change is
/// kept for `plugin::changes` to hand out.
pub(crate) fn hear(session: &mut Session) {
    // The last value of each parameter is the one that stands.
    let mut moved: Vec<(InsertId, u32, f64)> = Vec::new();
    for (id, slot) in &mut session.plugins {
        let PluginSlot::Loaded(plugin) = slot else {
            continue;
        };
        let Some(changes) = plugin.changes.as_mut() else {
            continue;
        };
        while let Some(change) = changes.pop() {
            if let PluginChange::Value { id: param, value } = change {
                match moved.iter_mut().find(|(i, p, _)| i == id && *p == param) {
                    Some(entry) => entry.2 = value,
                    None => moved.push((*id, param, value)),
                }
            }
            plugin.heard.push(change);
        }
        // Nobody is asking: keep the newest.
        let extra = plugin
            .heard
            .len()
            .saturating_sub(PluginNode::CHANGE_QUEUE_CAPACITY);
        plugin.heard.drain(..extra);
    }
    for (insert, param, value) in moved {
        let edit = commands::set_insert_param(insert, param, value);
        if crate::session::edit(session, edit).is_ok() {
            // The plugin has the value already: nothing to send it.
            note_sent(session, insert, param, value);
        }
    }
}

/// What a plugin has reported changing itself since this was last asked.
pub(crate) fn take_heard(session: &mut Session, insert: InsertId) -> Result<Vec<PluginChange>> {
    match session.plugins.iter_mut().find(|(id, _)| *id == insert) {
        Some((_, PluginSlot::Loaded(plugin))) => Ok(std::mem::take(&mut plugin.heard)),
        Some((_, PluginSlot::Missing(error))) => Err(error.clone()),
        None => Err(SuperbError::NotFound {
            kind: "loaded plugin",
            id: insert.raw(),
        }),
    }
}

/// A loaded plugin, by its insert.
pub(crate) fn loaded(session: &Session, insert: InsertId) -> Result<&LoadedPlugin> {
    match session.plugins.iter().find(|(id, _)| *id == insert) {
        Some((_, PluginSlot::Loaded(plugin))) => Ok(plugin),
        Some((_, PluginSlot::Missing(error))) => Err(error.clone()),
        None => Err(SuperbError::NotFound {
            kind: "loaded plugin",
            id: insert.raw(),
        }),
    }
}

/// Notes a parameter value as given to the plugin.
pub(crate) fn note_sent(session: &mut Session, insert: InsertId, param: u32, value: f64) {
    if let Some((_, PluginSlot::Loaded(plugin))) =
        session.plugins.iter_mut().find(|(id, _)| *id == insert)
    {
        match plugin.sent.iter_mut().find(|(p, _)| *p == param) {
            Some(entry) => entry.1 = value,
            None => plugin.sent.push((param, value)),
        }
    }
}
