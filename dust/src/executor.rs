use leptos::logging::log;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

use crate::*;

pub struct Executor<I, V> {
    callbacks: Vec<CallbackWithId<I, V>>,
    input_to_callbacks: HashMap<I, Vec<CallbackId>>,
    // Maps callback ids to the ids of callbacks that are immediately triggered by them.
    callback_to_dependents: HashMap<CallbackId, Vec<CallbackId>>,
    // Maps callback ids to their index in the topological sort.
    callback_to_topological_rank: HashMap<CallbackId, usize>,
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
            callback_to_dependents: HashMap::new(),
            callback_to_topological_rank: HashMap::new(),
        };
        return app;
    }

    pub fn register_callback(&mut self, callback: Callback<I, V>) {
        let id = CallbackId(self.callbacks.len());

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

        let mut callback_names: HashMap<CallbackId, &'static str> = HashMap::new();
        for cb in self.callbacks.iter() {
            callback_names.insert(cb.id, cb.callback.name);
            self.callback_to_dependents.insert(cb.id, Vec::new());
            let edges = self.callback_to_dependents.get_mut(&cb.id).unwrap();

            for output in cb.callback.outputs.iter() {
                if let Some(deps) = self.input_to_callbacks.get(output) {
                    for dep in deps.iter() {
                        edges.push(*dep);
                    }
                }
            }
        }

        let mut temp_marks: HashSet<CallbackId> = HashSet::new();
        let mut perm_marks: HashSet<CallbackId> = HashSet::new();
        let mut cycle: Vec<CallbackId> = Vec::new();
        let mut topological_order: Vec<CallbackId> = Vec::new();

        fn visit(
            id: CallbackId,
            callback_names: &HashMap<CallbackId, &'static str>,
            callback_to_dependents: &HashMap<CallbackId, Vec<CallbackId>>,
            temp_marks: &mut HashSet<CallbackId>,
            perm_marks: &mut HashSet<CallbackId>,
            cycle: &mut Vec<CallbackId>,
            topological_order: &mut Vec<CallbackId>,
        ) -> bool {
            if perm_marks.contains(&id) {
                return false;
            }
            if temp_marks.contains(&id) {
                cycle.push(id);
                return true;
            }

            temp_marks.insert(id);
            for dep in callback_to_dependents.get(&id).unwrap().iter() {
                if visit(
                    *dep,
                    callback_names,
                    callback_to_dependents,
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
                    &self.callback_to_dependents,
                    &mut temp_marks,
                    &mut perm_marks,
                    &mut cycle,
                    &mut topological_order,
                ) {
                    let cycle_description = cycle
                        .iter()
                        .rev()
                        .map(|id| self.callbacks[id.callback_index()].callback.name)
                        .collect::<Vec<&str>>()
                        .join(" -> ");
                    panic!("Found callback cycle: {}", cycle_description);
                }
            }
        }

        topological_order.reverse();
        let topological_order_description = topological_order
            .iter()
            .map(|id| self.callbacks[id.callback_index()].callback.name)
            .collect::<Vec<&str>>()
            .join(" -> ");
        log!(
            "Computed callback topological order {}",
            topological_order_description
        );

        for (rank, id) in topological_order.iter().enumerate() {
            self.callback_to_topological_rank.insert(*id, rank);
        }
    }

    pub fn get_execution_plan(&self, updated_inputs: &Vec<I>) -> Vec<CallbackId> {
        let mut execution_plan: Vec<CallbackId> = Vec::new();
        let mut visited_cb_ids: HashSet<CallbackId> = HashSet::new();

        fn visit(
            id: CallbackId,
            callback_to_dependents: &HashMap<CallbackId, Vec<CallbackId>>,
            execution_plan: &mut Vec<CallbackId>,
            visited_cb_ids: &mut HashSet<CallbackId>,
        ) {
            if visited_cb_ids.contains(&id) {
                return;
            }

            execution_plan.push(id);
            visited_cb_ids.insert(id);
            for dep in callback_to_dependents.get(&id).unwrap().iter() {
                visit(*dep, callback_to_dependents, execution_plan, visited_cb_ids);
            }
        }

        for input in updated_inputs.iter() {
            for id in self.input_to_callbacks.get(input).unwrap().iter() {
                visit(
                    *id,
                    &self.callback_to_dependents,
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
        execution_plan: &Vec<CallbackId>,
    ) -> HashSet<I> {
        let mut available_inputs: HashSet<I> = HashSet::from_iter(updated_inputs.iter().cloned());
        let mut required_state: HashSet<I> = HashSet::new();

        for id in execution_plan.iter() {
            let callback = &self.callbacks[id.callback_index()].callback;
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
            let callback = &self.callbacks[id.callback_index()].callback;

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