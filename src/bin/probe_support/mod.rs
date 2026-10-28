//! Shared engine adapter for the optional command-line diagnostic probes.
use opcda_egui::bff::model::{Command, Event};
use opclook_engine::Engine;
use std::cell::RefCell;

pub struct Probe(RefCell<Engine<()>>);
impl Probe {
    pub fn new() -> Self {
        Self(RefCell::new(Engine::default()))
    }
    pub fn send(&self, command: Command) -> Result<(), String> {
        let expected = command.request_id;
        let id = self
            .0
            .borrow_mut()
            .send(command.session, command.protocol, command.kind, ())
            .map_err(|error| format!("dispatch failed: {error:?}"))?;
        if Some(id) != expected {
            return Err("probe request sequence diverged".into());
        }
        Ok(())
    }
    pub fn try_recv(&self) -> Option<Event> {
        let mut engine = self.0.borrow_mut();
        loop {
            let event = engine.try_recv()?;
            if engine.accept(&event).is_ok() {
                return Some(event);
            }
        }
    }
}
