use glam::{Affine3A, Vec3, Vec3A};
use slotmap::{HopSlotMap, Key};

use crate::{
    overlays::{
        anchor::create_anchor, keyboard::builder::create_keyboard, screen::create_screens,
        watch::create_watch,
    },
    state::AppState,
    windowing::{
        set::OverlayWindowSet, snap_upright, window::OverlayWindowData, OverlayID, OverlaySelector,
    },
};

pub struct OverlayWindowManager<T> {
    overlays: HopSlotMap<OverlayID, OverlayWindowData<T>>,
    sets: Vec<OverlayWindowSet>,
    /// The set that is currently visible.
    current_set: Option<usize>,
    /// The set that will be restored by show_hide.
    /// Usually the same as current_set, except it keeps its value when current_set is hidden.
    restore_set: usize,
    anchor_local: Affine3A,
    watch_id: OverlayID,
}

impl<T> OverlayWindowManager<T>
where
    T: Default,
{
    pub fn new(app: &mut AppState, headless: bool) -> anyhow::Result<Self> {
        let mut maybe_keymap = None;

        let mut me = Self {
            overlays: HopSlotMap::with_key(),
            current_set: Some(0),
            restore_set: 0,
            sets: vec![OverlayWindowSet::default()],
            anchor_local: Affine3A::from_translation(Vec3::NEG_Z),
            watch_id: OverlayID::null(), // set down below
        };

        if headless {
            log::info!("Running in headless mode; keyboard will be en-US");
        } else {
            // create one work set for each screen.
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

        let watch = OverlayWindowData::from_config(create_watch(app, me.sets.len())?);
        me.watch_id = me.add(watch, app);

        me.switch_to_set(app, None);

        Ok(me)
    }
}

impl<T> OverlayWindowManager<T> {
    pub fn mut_by_selector(
        &mut self,
        selector: &OverlaySelector,
    ) -> Option<&mut OverlayWindowData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.mut_by_id(*id),
            OverlaySelector::Name(name) => self.lookup(name).and_then(|id| self.mut_by_id(id)),
        }
    }

    pub fn remove_by_selector(
        &mut self,
        selector: &OverlaySelector,
    ) -> Option<OverlayWindowData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.overlays.remove(*id),
            OverlaySelector::Name(name) => {
                self.lookup(name).and_then(|id| self.overlays.remove(id))
            }
        }
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
        if overlay.config.show_on_spawn {
            overlay.config.activate(app);
        }
        
        self.overlays.insert(overlay)
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
                if let Some(mut state) = data.config.active_state.take() {
                    if let Some(transform) = data.config.saved_transform.take() {
                        state.transform = transform;
                    } else {
                        state.transform = Affine3A::ZERO;
                    }
                    log::debug!("{}: active_state → ws{}", data.config.name, current_set);
                    ws.overlays.insert(id, state);
                }
            }
        }

        if let Some(new_set) = new_set {
            if new_set >= self.sets.len() {
                log::warn!("switch_to_set: new_set is out of range ({new_set:?})");
                return;
            }

            let ws = &mut self.sets[new_set];
            for (id, data) in self.overlays.iter_mut().filter(|(_, d)| !d.config.global) {
                if let Some(mut state) = ws.overlays.remove(id) {
                    if state.transform.x_axis.length_squared() > f32::EPSILON {
                        data.config.saved_transform = Some(state.transform);
                    }
                    state.transform = Affine3A::IDENTITY;
                    log::debug!("{}: ws{} → active_state", data.config.name, new_set);
                    data.config.active_state = Some(state);
                    data.config.reset(app, false);
                }
            }
            self.restore_set = new_set;
        }
        self.current_set = new_set;
    }

    pub fn show_hide(&mut self, app: &mut AppState) {
        if self.current_set.is_none() {
            let hmd = snap_upright(app.input_state.hmd, Vec3A::Y);
            app.anchor = hmd * self.anchor_local;

            self.switch_to_set(app, Some(self.restore_set));
        } else {
            self.switch_to_set(app, None);
        }

        // toggle watch back on if it was hidden
        self.mut_by_id(self.watch_id).unwrap().config.activate(app);
    }
}
