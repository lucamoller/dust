use leptos::logging::log;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

pub mod file_handler;
pub mod serve;
mod context;
pub use context::*;


pub use dust_macro::{
    dust_define_client_callback, dust_define_server_callback, dust_lib, dust_main, DustState,
};

// Re-exports
pub use console_error_panic_hook;
pub use leptos;
pub use leptos_meta;
pub use leptos_router;
pub use once_cell;
pub use serde;

#[cfg(feature = "ssr")]
pub use tokio;

#[cfg(feature = "hydrate")]
pub use wasm_bindgen;

pub use web_sys;

pub struct Input<T> {
    pub value: T,
}

#[derive(Clone, Copy)]
pub enum OutputState<T> {
    NoChange,
    Updated(T),
}

pub struct Output<T> {
    pub state: OutputState<T>,
}

impl<T: Clone> Output<T> {
    pub fn new() -> Output<T> {
        Output {
            state: OutputState::NoChange,
        }
    }

    pub fn set(&mut self, value: T) {
        self.state = OutputState::Updated(value);
    }
}

#[derive(Clone)]
pub enum CallbackType {
    Server,
    Client,
}

#[derive(Clone)]
pub struct Callback<I, V> {
    pub name: &'static str,
    pub cb: Option<fn(&HashMap<I, V>) -> Vec<V>>,
    pub inputs: Vec<I>,
    pub outputs: Vec<I>,
    pub cb_type: CallbackType,
}

impl<I, V> Callback<I, V> {
    pub fn new(
        name: &'static str,
        cb: Option<fn(&HashMap<I, V>) -> Vec<V>>,
        inputs: Vec<I>,
        outputs: Vec<I>,
        cb_type: CallbackType,
    ) -> Self {
        Self {
            name,
            cb,
            inputs,
            outputs,
            cb_type,
        }
    }
}

impl<I, V> std::hash::Hash for Callback<I, V> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.name.hash(state);
        state.finish();
    }
}

impl<I, V> PartialEq for Callback<I, V> {
    fn eq(&self, other: &Callback<I, V>) -> bool {
        return self.name == other.name;
    }
}

#[derive(Clone)]
pub struct CallbackWithId<I, V> {
    pub id: usize,
    pub callback: Callback<I, V>,
}

pub trait StateTypes {
    type Identifier;
    type Value;
}

impl<I, V> Eq for Callback<I, V> {}

pub trait ValueToIdentifier<I> {
    fn to_identifier(&self) -> I;
}

pub struct Executor<I, V> {
    callbacks: Vec<CallbackWithId<I, V>>,
    input_to_callbacks: HashMap<I, Vec<usize>>,
    // Maps callback ids to the ids of callbacks that are immediately triggered by them.
    callback_to_dependants: HashMap<usize, Vec<usize>>,
    // Maps callback ids to their index in the topological sort.
    callback_to_topological_rank: HashMap<usize, usize>,
}

impl<I, V> Executor<I, V>
where
    I: Hash + PartialEq + Eq + Clone + Copy,
    V: Clone + std::fmt::Debug + ValueToIdentifier<I>,
{
    pub fn new() -> Executor<I, V> {
        let app = Executor {
            callbacks: Vec::new(),
            input_to_callbacks: HashMap::new(),
            callback_to_dependants: HashMap::new(),
            callback_to_topological_rank: HashMap::new(),
        };
        return app;
    }

    pub fn register_callback(&mut self, callback: Callback<I, V>) {
        let id = self.callbacks.len();

        self.callbacks.push(CallbackWithId {
            id: id,
            callback: callback,
        });
    }

    pub fn init_callbacks(&mut self) {
        for cb in self.callbacks.iter() {
            for input in cb.callback.inputs.iter() {
                if !self.input_to_callbacks.contains_key(input) {
                    self.input_to_callbacks.insert(input.clone(), Vec::new());
                }

                let v = self.input_to_callbacks.get_mut(input).unwrap();
                v.push(cb.id);
            }
        }

        let mut callback_names: HashMap<usize, &'static str> = HashMap::new();
        for cb in self.callbacks.iter() {
            callback_names.insert(cb.id, cb.callback.name);
            self.callback_to_dependants.insert(cb.id, Vec::new());
            let edges = self.callback_to_dependants.get_mut(&cb.id).unwrap();

            for output in cb.callback.outputs.iter() {
                if let Some(deps) = self.input_to_callbacks.get(output) {
                    for dep in deps.iter() {
                        edges.push(*dep);
                    }
                }
            }
        }

        let mut temp_marks: HashSet<usize> = HashSet::new();
        let mut perm_marks: HashSet<usize> = HashSet::new();
        let mut cycle: Vec<usize> = Vec::new();
        let mut topological_order: Vec<usize> = Vec::new();

        fn visit(
            id: usize,
            callback_names: &HashMap<usize, &'static str>,
            callback_to_dependants: &HashMap<usize, Vec<usize>>,
            temp_marks: &mut HashSet<usize>,
            perm_marks: &mut HashSet<usize>,
            cycle: &mut Vec<usize>,
            topological_order: &mut Vec<usize>,
        ) -> bool {
            if perm_marks.contains(&id) {
                return false;
            }
            if temp_marks.contains(&id) {
                cycle.push(id);
                return true;
            }

            temp_marks.insert(id);
            for dep in callback_to_dependants.get(&id).unwrap().iter() {
                if visit(
                    *dep,
                    callback_names,
                    callback_to_dependants,
                    temp_marks,
                    perm_marks,
                    cycle,
                    topological_order,
                ) {
                    if cycle.len() == 1 || cycle.first().unwrap() != cycle.last().unwrap() {
                        // Stop adding nodes when there's a complete cycle.
                        cycle.push(id);
                    }
                    return true;
                }
            }
            temp_marks.remove(&id);
            perm_marks.insert(id);
            topological_order.push(id);
            return false;
        }

        for cb in self.callbacks.iter() {
            if !perm_marks.contains(&cb.id) {
                if visit(
                    cb.id,
                    &callback_names,
                    &self.callback_to_dependants,
                    &mut temp_marks,
                    &mut perm_marks,
                    &mut cycle,
                    &mut topological_order,
                ) {
                    let cycle_description = cycle
                        .iter()
                        .rev()
                        .map(|id| self.callbacks[*id].callback.name)
                        .collect::<Vec<&str>>()
                        .join(" -> ");
                    panic!("Found callback cycle: {}", cycle_description);
                }
            }
        }

        topological_order.reverse();
        let topological_order_description = topological_order
            .iter()
            .map(|id| self.callbacks[*id].callback.name)
            .collect::<Vec<&str>>()
            .join(" -> ");
        log!(
            "Computed callback tological order {}",
            topological_order_description
        );

        for (rank, id) in topological_order.iter().enumerate() {
            self.callback_to_topological_rank.insert(*id, rank);
        }
    }

    pub fn get_execution_plan(&self, updated_inputs: &Vec<I>) -> Vec<usize> {
        let mut execution_plan: Vec<usize> = Vec::new();
        let mut visited_cb_ids: HashSet<usize> = HashSet::new();

        fn visit(
            id: usize,
            callback_to_dependants: &HashMap<usize, Vec<usize>>,
            execution_plan: &mut Vec<usize>,
            visited_cb_ids: &mut HashSet<usize>,
        ) {
            if visited_cb_ids.contains(&id) {
                return;
            }

            execution_plan.push(id);
            visited_cb_ids.insert(id);
            for dep in callback_to_dependants.get(&id).unwrap().iter() {
                visit(*dep, callback_to_dependants, execution_plan, visited_cb_ids);
            }
        }

        for input in updated_inputs.iter() {
            for id in self.input_to_callbacks.get(input).unwrap().iter() {
                visit(
                    *id,
                    &self.callback_to_dependants,
                    &mut execution_plan,
                    &mut visited_cb_ids,
                );
            }
        }

        execution_plan.sort_by_key(|id| self.callback_to_topological_rank.get(id).unwrap());
        return execution_plan;
    }

    pub fn get_required_state(
        &self,
        updated_inputs: &Vec<I>,
        execution_plan: &Vec<usize>,
    ) -> HashSet<I> {
        let mut available_inputs: HashSet<I> = HashSet::from_iter(updated_inputs.iter().cloned());
        let mut required_state: HashSet<I> = HashSet::new();

        for id in execution_plan.iter() {
            let callback = &self.callbacks[*id].callback;
            for input in callback.inputs.iter() {
                if !available_inputs.contains(input) {
                    required_state.insert(*input);
                }
            }
            for output in callback.outputs.iter() {
                available_inputs.insert(*output);
            }
        }

        return required_state;
    }

    pub fn get_required_initialization_inputs(&self) -> HashSet<I> {
        let mut required_inputs: HashSet<I> = HashSet::new();
        for cb in self.callbacks.iter() {
            for input in cb.callback.inputs.iter() {
                required_inputs.insert(input.clone());
            }
        }

        for cb in self.callbacks.iter() {
            for output in cb.callback.outputs.iter() {
                // An input is not required if it's an output of another method
                // (which means it will be computed during initialization).
                if required_inputs.contains(output) {
                    required_inputs.remove(output);
                }
            }
        }
        return required_inputs;
    }

    pub fn process_updates(&self, input_updates: Vec<V>, required_state: Vec<V>) -> Vec<V> {
        let mut state: HashMap<I, V> = HashMap::new();

        for value in required_state.iter() {
            let identifier = value.to_identifier();
            state.insert(identifier, value.clone());
        }
        for value in input_updates.iter() {
            let identifier = value.to_identifier();
            state.insert(identifier, value.clone());
        }

        let updated_inputs = input_updates.iter().map(|v| v.to_identifier()).collect();
        let execution_plan = self.get_execution_plan(&updated_inputs);

        let mut output_updates: Vec<V> = Vec::new();
        for id in execution_plan.iter() {
            let callback = &self.callbacks[*id].callback;

            let mut new_updates = (callback.cb.unwrap())(&state);
            for value in new_updates.iter() {
                let identifier = value.to_identifier();
                state.insert(identifier, value.clone());
            }
            output_updates.append(&mut new_updates);
        }

        println!("output_updates: {:?}", output_updates);
        return output_updates;
    }
}
