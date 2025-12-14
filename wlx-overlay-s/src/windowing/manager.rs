use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::Ordering,
};

use glam::{Affine3A, Vec3, Vec3A};
use slotmap::{HopSlotMap, Key, SecondaryMap};
use wlx_common::{
    astr_containers::{AStrMap, AStrMapExt},
    config::SerializedWindowSet,
    overlays::ToastTopic,
};

use crate::{
    FRAME_COUNTER,
    backend::task::OverlayTask,
    overlays::{
        anchor::create_anchor, edit::EditWrapperManager, keyboard::create_keyboard,
        screen::create_screens, toast::Toast, watch::create_watch,
    },
    state::AppState,
    windowing::{
        OverlayID, OverlaySelector,
        backend::{OverlayEventData, OverlayMeta},
        set::OverlayWindowSet,
        snap_upright,
        window::{OverlayCategory, OverlayWindowData},
    },
};

pub const MAX_OVERLAY_SETS: usize = 7;

pub struct OverlayWindowManager<T> {
    wrappers: EditWrapperManager,
    overlays: HopSlotMap<OverlayID, OverlayWindowData<T>>,
    sets: Vec<OverlayWindowSet>,
    /// The set that is currently visible.
    current_set: Option<usize>,
    /// The set that will be restored by show_hide.
    /// Usually the same as current_set, except it keeps its value when current_set is hidden.
    restore_set: usize,
    anchor_local: Affine3A,
    watch_id: OverlayID,
    edit_mode: bool,
    dropped_overlays: VecDeque<OverlayWindowData<T>>,
}

impl<T> OverlayWindowManager<T>
where
    T: Default,
{
    pub fn new(app: &mut AppState, headless: bool) -> anyhow::Result<Self> {
        let mut maybe_keymap = None;

        let mut me = Self {
            wrappers: EditWrapperManager::default(),
            overlays: HopSlotMap::with_key(),
            current_set: Some(0),
            restore_set: 0,
            sets: vec![OverlayWindowSet::default()],
            anchor_local: Affine3A::from_translation(Vec3::NEG_Z),
            watch_id: OverlayID::null(), // set down below
            edit_mode: false,
            dropped_overlays: VecDeque::with_capacity(8),
        };

        if headless {
            log::info!("Running in headless mode; keyboard will be en-US");
        } else {
            // create one window set for each screen.
            // this is the default and would be overwritten by
            // OverlayWindowManager::restore_layout down below
            match create_screens(app) {
                Ok((data, keymap)) => {
                    let last_idx = data.screens.len() - 1;
                    for (idx, (meta, mut config)) in data.screens.into_iter().enumerate() {
                        config.show_on_spawn = true;
                        me.add(OverlayWindowData::from_config(config), app);

                        if idx < last_idx {
                            me.sets.push(OverlayWindowSet::default());
                            me.switch_to_set(app, Some(me.current_set.unwrap() + 1));
                        }
                        app.screens.push(meta);
                    }

                    maybe_keymap = keymap;
                }
                Err(e) => log::error!("Unable to initialize screens: {e:?}"),
            }
        }

        let mut keyboard = OverlayWindowData::from_config(create_keyboard(app, maybe_keymap)?);
        keyboard.config.show_on_spawn = true;
        let keyboard_id = me.add(keyboard, app);

        // is this needed?
        me.switch_to_set(app, None);

        // copy keyboard to all sets
        let kbd_state = me
            .sets
            .last()
            .and_then(|s| s.overlays.get(keyboard_id))
            .unwrap()
            .clone();
        for set in &mut me.sets {
            set.overlays.insert(keyboard_id, kbd_state.clone());
        }

        let anchor = OverlayWindowData::from_config(create_anchor(app)?);
        me.add(anchor, app);

        let watch = OverlayWindowData::from_config(create_watch(app)?);
        me.watch_id = me.add(watch, app);

        // overwrite default layout with saved layout, if exists
        me.restore_layout(app);
        me.overlays_changed(app)?;

        for ev in [
            OverlayEventData::NumSetsChanged(me.sets.len()),
            OverlayEventData::EditModeChanged(false),
            OverlayEventData::DevicesChanged,
        ] {
            me.mut_by_id(me.watch_id)
                .unwrap()
                .config
                .backend
                .notify(app, ev)?;
        }

        Ok(me)
    }

    pub fn handle_task(&mut self, app: &mut AppState, task: OverlayTask) -> anyhow::Result<()> {
        match task {
            OverlayTask::ShowHide => self.show_hide(app),
            OverlayTask::ToggleSet(set) => {
                self.switch_or_toggle_set(app, set);
            }
            OverlayTask::ToggleEditMode => {
                self.set_edit_mode(!self.edit_mode, app)?;
            }
            OverlayTask::AddSet => {
                self.sets.push(OverlayWindowSet::default());
                let len = self.sets.len();
                if let Some(watch) = self.mut_by_id(self.watch_id) {
                    watch
                        .config
                        .backend
                        .notify(app, OverlayEventData::NumSetsChanged(len))?;
                }
            }
            OverlayTask::DeleteActiveSet => {
                let Some(set) = self.current_set else {
                    Toast::new(
                        ToastTopic::System,
                        "Can't remove set".into(),
                        "No set is selected!".into(),
                    )
                    .with_timeout(5.)
                    .with_sound(true)
                    .submit(app);
                    return Ok(());
                };

                if self.sets.len() <= 1 {
                    Toast::new(
                        ToastTopic::System,
                        "Can't remove set".into(),
                        "This is the last existing set!".into(),
                    )
                    .with_timeout(5.)
                    .with_sound(true)
                    .submit(app);
                    return Ok(());
                }

                self.switch_to_set(app, None);
                self.sets.remove(set);
                let len = self.sets.len();
                if let Some(watch) = self.mut_by_id(self.watch_id) {
                    watch
                        .config
                        .backend
                        .notify(app, OverlayEventData::NumSetsChanged(len))?;
                }
            }
            OverlayTask::CleanupMirrors => {
                let mut ids_to_remove = vec![];
                for (oid, o) in &self.overlays {
                    if !matches!(o.config.category, OverlayCategory::Mirror) {
                        continue;
                    }
                    if o.config.active_state.is_some() {
                        continue;
                    }
                    ids_to_remove.push(oid);
                }

                for oid in ids_to_remove {
                    self.remove_by_selector(&OverlaySelector::Id(oid), app);
                }
            }
            OverlayTask::Modify(sel, f) => {
                if let Some(o) = self.mut_by_selector(&sel) {
                    f(app, &mut o.config);
                } else {
                    log::warn!("Overlay not found for task: {sel:?}");
                }
            }
            OverlayTask::Create(sel, f) => {
                let None = self.mut_by_selector(&sel) else {
                    log::debug!("Could not create {sel:?}: exists");
                    return Ok(());
                };

                let Some(overlay_config) = f(app) else {
                    log::debug!("Could not create {sel:?}: empty config");
                    return Ok(());
                };

                self.add(
                    OverlayWindowData {
                        birthframe: FRAME_COUNTER.load(Ordering::Relaxed),
                        ..OverlayWindowData::from_config(overlay_config)
                    },
                    app,
                );
            }
            OverlayTask::Drop(sel) => {
                if let Some(o) = self.mut_by_selector(&sel)
                    && o.birthframe < FRAME_COUNTER.load(Ordering::Relaxed)
                    && let Some(o) = self.remove_by_selector(&sel, app)
                {
                    log::debug!("Dropping overlay {}", o.config.name);
                    self.dropped_overlays.push_back(o);
                }
            }
        }
        Ok(())
    }
}

impl<T> OverlayWindowManager<T> {
    pub fn pop_dropped(&mut self) -> Option<OverlayWindowData<T>> {
        self.dropped_overlays.pop_front()
    }

    pub fn persist_layout(&mut self, app: &mut AppState) {
        app.session.config.global_set.clear();
        app.session.config.sets.clear();
        app.session.config.sets.reserve(self.sets.len());
        app.session.config.last_set = self.restore_set as _;

        // only safe to save when current_set is None
        let restore_after = if self.current_set.is_some() {
            self.switch_to_set(app, None);
            true
        } else {
            false
        };

        for set in &self.sets {
            let mut overlays: HashMap<_, _> = set
                .overlays
                .iter()
                .filter_map(|(k, v)| {
                    let n = self.overlays.get(k).map(|o| o.config.name.clone())?;
                    Some((n, v.clone()))
                })
                .collect();

            // overlays that we haven't seen since startup (e.g. wayvr apps)
            for (k, o) in &set.inactive_overlays {
                if !overlays.contains_key(k) {
                    overlays.insert(k.clone(), o.clone());
                }
            }

            let serialized = SerializedWindowSet {
                name: set.name.clone(),
                overlays,
            };
            app.session.config.sets.push(serialized);
        }

        // global overlays; watch, toast
        for oid in &[self.watch_id] {
            let Some(o) = self.get_by_id(*oid) else {
                break;
            };
            let Some(state) = o.config.active_state.clone() else {
                break;
            };
            app.session
                .config
                .global_set
                .insert(o.config.name.clone(), state.clone());
        }

        if restore_after {
            self.switch_to_set(app, Some(self.restore_set));
        }
    }

    pub fn restore_layout(&mut self, app: &mut AppState) {
        if app.session.config.sets.is_empty() {
            // keep defaults
            return;
        }

        // only safe to load when current_set is None
        if self.current_set.is_some() {
            self.switch_to_set(app, None);
        }

        self.sets.clear();
        self.sets.reserve(app.session.config.sets.len());

        for (i, s) in app.session.config.sets.iter().enumerate() {
            let mut overlays = SecondaryMap::new();
            let mut inactive_overlays = AStrMap::new();

            for (name, o) in &s.overlays {
                if let Some(id) = self.lookup(name) {
                    log::debug!("set {i}: loaded state for {name}");
                    overlays.insert(id, o.clone());
                } else {
                    log::debug!(
                        "set {i} has saved state for {name} which doesn't exist. will apply state once added."
                    );
                    inactive_overlays.arc_set(name.clone(), o.clone());
                }
            }

            self.sets.push(OverlayWindowSet {
                name: s.name.clone(),
                overlays,
                inactive_overlays,
            });
        }

        // global overlays
        for oid in &[self.watch_id] {
            if let Some(o) = self.mut_by_id(*oid) {
                if let Some(state) = app.session.config.global_set.get(&*o.config.name).cloned() {
                    o.config.active_state = Some(state);
                    o.config.reset(app, false);
                    log::debug!("global set: loaded state for {}", o.config.name);
                } else {
                    log::debug!("global set: no state for {}", o.config.name);
                }
            }
        }

        self.restore_set = (app.session.config.last_set as usize).min(self.sets.len() - 1);
    }

    pub const fn get_edit_mode(&self) -> bool {
        self.edit_mode
    }

    pub fn set_edit_mode(&mut self, enabled: bool, app: &mut AppState) -> anyhow::Result<()> {
        let changed = enabled != self.edit_mode;
        self.edit_mode = enabled;
        if !enabled {
            for o in self.overlays.values_mut() {
                self.wrappers.unwrap_edit_mode(&mut o.config);
            }
        }
        if changed && let Some(watch) = self.mut_by_id(self.watch_id) {
            watch
                .config
                .active_state
                .iter_mut()
                .for_each(|f| f.grabbable = enabled);
            watch
                .config
                .backend
                .notify(app, OverlayEventData::EditModeChanged(enabled))?;
        }
        Ok(())
    }

    pub fn edit_overlay(&mut self, id: OverlayID, enabled: bool, app: &mut AppState) {
        if !self.edit_mode {
            return;
        }

        let Some(overlay) = self.overlays.get_mut(id) else {
            return;
        };

        if overlay.config.global {
            // watch, anchor, toast, dashboard
            return;
        }

        if enabled {
            self.wrappers
                .wrap_edit_mode(id, &mut overlay.config, app)
                .unwrap(); // FIXME: unwrap
        } else {
            self.wrappers.unwrap_edit_mode(&mut overlay.config);
        }
    }

    pub fn mut_by_selector(
        &mut self,
        selector: &OverlaySelector,
    ) -> Option<&mut OverlayWindowData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.mut_by_id(*id),
            OverlaySelector::Name(name) => self.lookup(name).and_then(|id| self.mut_by_id(id)),
        }
    }

    fn remove_by_selector(
        &mut self,
        selector: &OverlaySelector,
        app: &mut AppState,
    ) -> Option<OverlayWindowData<T>> {
        let id = match selector {
            OverlaySelector::Id(id) => *id,
            OverlaySelector::Name(name) => self.lookup(name)?,
        };

        let ret_val = self.overlays.remove(id);
        let internal = ret_val
            .as_ref()
            .is_some_and(|o| matches!(o.config.category, OverlayCategory::Internal));

        if !internal && let Err(e) = self.overlays_changed(app) {
            log::error!("Error while removing overlay: {e:?}");
        }

        ret_val
    }

    pub fn get_by_id(&mut self, id: OverlayID) -> Option<&OverlayWindowData<T>> {
        self.overlays.get(id)
    }

    pub fn mut_by_id(&mut self, id: OverlayID) -> Option<&mut OverlayWindowData<T>> {
        self.overlays.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (OverlayID, &'_ OverlayWindowData<T>)> {
        self.overlays.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (OverlayID, &'_ mut OverlayWindowData<T>)> {
        self.overlays.iter_mut()
    }

    pub fn values(&self) -> impl Iterator<Item = &'_ OverlayWindowData<T>> {
        self.overlays.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &'_ mut OverlayWindowData<T>> {
        self.overlays.values_mut()
    }

    pub fn lookup(&self, name: &str) -> Option<OverlayID> {
        self.overlays
            .iter()
            .find(|(_, v)| v.config.name.as_ref() == name)
            .map(|(k, _)| k)
    }

    pub fn add(&mut self, mut overlay: OverlayWindowData<T>, app: &mut AppState) -> OverlayID {
        while self.lookup(&overlay.config.name).is_some() {
            log::error!(
                "An overlay with name {} already exists. Deduplicating, but things may break!",
                overlay.config.name
            );
            overlay.config.name = format!("{}_2", overlay.config.name).into();
        }

        let name = overlay.config.name.clone();
        let global = overlay.config.global;
        let internal = matches!(overlay.config.category, OverlayCategory::Internal);
        let show_on_spawn = overlay.config.show_on_spawn;

        let oid = self.overlays.insert(overlay);
        let mut shown = false;

        if !global {
            for (i, set) in self.sets.iter_mut().enumerate() {
                let Some(state) = set.inactive_overlays.arc_rm(&name) else {
                    continue;
                };
                if self.current_set == Some(i) {
                    let o = &mut self.overlays[oid];
                    o.config.active_state = Some(state);
                    o.config.reset(app, false);
                    shown = true;
                    log::debug!("loaded state for {name} to active set!");
                } else {
                    set.overlays.insert(oid, state);
                    log::debug!("loaded state for {name} to set {i}");
                }
            }
        }

        if !shown && show_on_spawn {
            log::debug!("activating {name} due to show_on_spawn");
            self.overlays[oid].config.activate(app);
        }
        if !internal && let Err(e) = self.overlays_changed(app) {
            log::error!("Error while adding overlay: {e:?}");
        }
        oid
    }

    pub fn switch_or_toggle_set(&mut self, app: &mut AppState, set: usize) {
        let new_set = if self.current_set.iter().any(|cur| *cur == set) {
            None
        } else {
            Some(set)
        };

        self.switch_to_set(app, new_set);
    }

    pub fn switch_to_set(&mut self, app: &mut AppState, new_set: Option<usize>) {
        if new_set == self.current_set {
            return;
        }

        if let Some(current_set) = self.current_set.as_ref() {
            let ws = &mut self.sets[*current_set];
            ws.overlays.clear();
            for (id, data) in self.overlays.iter_mut().filter(|(_, d)| !d.config.global) {
                if let Some(state) = data.config.active_state.take() {
                    log::debug!("{}: active_state → ws{}", data.config.name, current_set);
                    ws.overlays.insert(id, state);
                }
            }
        }

        if let Some(new_set) = new_set {
            if new_set >= self.sets.len() {
                log::error!("switch_to_set: new_set is out of range ({new_set:?})");
                return;
            }

            let ws = &mut self.sets[new_set];
            for (id, data) in self.overlays.iter_mut().filter(|(_, d)| !d.config.global) {
                if let Some(state) = ws.overlays.remove(id) {
                    log::debug!("{}: ws{} → active_state", data.config.name, new_set);
                    data.config.active_state = Some(state);
                    data.config.reset(app, false);
                }
            }
            self.restore_set = new_set;
        }
        self.current_set = new_set;

        if let Some(watch) = self.mut_by_id(self.watch_id) {
            watch
                .config
                .backend
                .notify(app, OverlayEventData::ActiveSetChanged(new_set))
                .unwrap(); // TODO: handle this
        }
    }

    pub fn show_hide(&mut self, app: &mut AppState) {
        if self.current_set.is_none() {
            let hmd = snap_upright(app.input_state.hmd, Vec3A::Y);
            app.anchor = hmd * self.anchor_local;

            self.switch_to_set(app, Some(self.restore_set));
        } else {
            self.switch_to_set(app, None);
        }
    }

    fn overlays_changed(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        let mut meta = Vec::with_capacity(self.overlays.len());
        for (id, data) in &self.overlays {
            if matches!(data.config.category, OverlayCategory::Internal) {
                continue;
            }
            meta.push(OverlayMeta {
                id,
                name: data.config.name.clone(),
                category: data.config.category,
            });
        }

        if let Some(watch) = self.mut_by_id(self.watch_id) {
            watch
                .config
                .backend
                .notify(app, OverlayEventData::OverlaysChanged(meta))?;
        }

        Ok(())
    }

    pub fn devices_changed(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        if let Some(watch) = self.mut_by_id(self.watch_id) {
            watch
                .config
                .backend
                .notify(app, OverlayEventData::DevicesChanged)?;
        }

        Ok(())
    }
}
