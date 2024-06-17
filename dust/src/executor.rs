use leptos::logging::log;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

use crate::*;

pub struct CallbacksContainer<I, V> {
    callbacks: Vec<CallbackWithId<I, V>>,
}

impl<I, V> CallbacksContainer<I, V> {
    pub fn new() -> CallbacksContainer<I, V> {
        return CallbacksContainer {
            callbacks: Vec::new(),
        };
    }

    pub fn add_callback(&mut self, callback: Callback<I, V>) {
        let id = CallbackId(self.callbacks.len());

        self.callbacks.push(CallbackWithId {
            id: id,
            callback: callback,
        });
    }

    pub fn get_callback(&self, id: &CallbackId) -> &Callback<I, V> {
        return &self.callbacks[id.callback_index()].callback;
    }

    pub fn all_callbacks(&self) -> &Vec<CallbackWithId<I, V>> {
        return &self.callbacks;
    }
}

pub struct Executor<I, V> {
    callbacks_container: CallbacksContainer<I, V>,
    input_to_callbacks: HashMap<I, Vec<CallbackId>>,
    // Maps callback ids to the ids of callbacks that are immediately triggered by them.
    callback_to_dependents: HashMap<CallbackId, Vec<CallbackId>>,
    // Maps callback ids to the ids of callbacks that can immediately trigger them.
    callback_to_ancestors: HashMap<CallbackId, Vec<CallbackId>>,
    // Maps callback ids to their index in the topological sort.
    callback_to_topological_rank: HashMap<CallbackId, usize>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    // Sequence of callbacks that should be executed in the client before the server callbacks.
    pub client_pre_plan: Vec<CallbackId>,
    // Sequence of callbacks that should be executed in the server.
    pub server_plan: Vec<CallbackId>,
    // Sequence of callbacks that should be executed in the client after the server callbacks.
    pub client_post_plan: Vec<CallbackId>,
}

impl ExecutionPlan {
    pub fn new() -> ExecutionPlan {
        ExecutionPlan {
            client_pre_plan: Vec::new(),
            server_plan: Vec::new(),
            client_post_plan: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ArgState {
    Unmodified,
    Updated,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ExecutionArg<V> {
    pub value: V,
    pub state: ArgState,
}

impl<I, V> Executor<I, V>
where
    I: Hash + PartialEq + Eq + Clone + Copy,
    V: Clone + std::fmt::Debug + ValueToIdentifier<I>,
{
    pub fn new() -> Executor<I, V> {
        let app = Executor {
            callbacks_container: CallbacksContainer::new(),
            input_to_callbacks: HashMap::new(),
            callback_to_dependents: HashMap::new(),
            callback_to_ancestors: HashMap::new(),
            callback_to_topological_rank: HashMap::new(),
        };
        return app;
    }

    pub fn register_callback(&mut self, callback: Callback<I, V>) {
        self.callbacks_container.add_callback(callback);
    }

    pub fn init_callbacks(&mut self) {
        for cb in self.callbacks_container.all_callbacks().iter() {
            for input in cb.callback.inputs.iter() {
                if !self.input_to_callbacks.contains_key(input) {
                    self.input_to_callbacks.insert(input.clone(), Vec::new());
                }

                let v = self.input_to_callbacks.get_mut(input).unwrap();
                v.push(cb.id);
            }
        }

        let mut callback_names: HashMap<CallbackId, &'static str> = HashMap::new();
        for cb in self.callbacks_container.all_callbacks().iter() {
            callback_names.insert(cb.id, cb.callback.name);
            self.callback_to_dependents.insert(cb.id, Vec::new());
            let dependents = self.callback_to_dependents.get_mut(&cb.id).unwrap();

            for output in cb.callback.outputs.iter() {
                if let Some(deps) = self.input_to_callbacks.get(output) {
                    for dep in deps.iter() {
                        dependents.push(*dep);

                        let ancestors = match self.callback_to_ancestors.get_mut(dep) {
                            Some(ancestors) => ancestors,
                            None => {
                                self.callback_to_ancestors.insert(*dep, Vec::new());
                                self.callback_to_ancestors.get_mut(dep).unwrap()
                            }
                        };
                        ancestors.push(cb.id);
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

        for cb in self.callbacks_container.all_callbacks().iter() {
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
                        .map(|id| self.callbacks_container.get_callback(id).name)
                        .collect::<Vec<&str>>()
                        .join(" -> ");
                    panic!("Found callback cycle: {}", cycle_description);
                }
            }
        }

        topological_order.reverse();
        let topological_order_description = topological_order
            .iter()
            .map(|id| self.callbacks_container.get_callback(id).name)
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

    pub fn get_execution_plan(&self, updated_inputs: &HashSet<I>) -> ExecutionPlan {
        let mut full_plan: Vec<CallbackId> = Vec::new();
        let mut visited_cb_ids: HashSet<CallbackId> = HashSet::new();

        fn visit(
            id: CallbackId,
            callback_to_dependents: &HashMap<CallbackId, Vec<CallbackId>>,
            full_plan: &mut Vec<CallbackId>,
            visited_cb_ids: &mut HashSet<CallbackId>,
        ) {
            if visited_cb_ids.contains(&id) {
                return;
            }

            full_plan.push(id);
            visited_cb_ids.insert(id);
            for dep in callback_to_dependents.get(&id).unwrap().iter() {
                visit(*dep, callback_to_dependents, full_plan, visited_cb_ids);
            }
        }

        for input in updated_inputs.iter() {
            for id in self.input_to_callbacks.get(input).unwrap().iter() {
                visit(
                    *id,
                    &self.callback_to_dependents,
                    &mut full_plan,
                    &mut visited_cb_ids,
                );
            }
        }

        full_plan.sort_by_key(|id| self.callback_to_topological_rank.get(id).unwrap());

        fn is_client_only<I, V>(
            id: &CallbackId,
            callbacks_container: &CallbacksContainer<I, V>,
            callbacks_in_plan: &HashSet<CallbackId>,
            callback_to_edges: &HashMap<CallbackId, Vec<CallbackId>>,
            visited: &mut HashMap<CallbackId, bool>,
        ) -> bool {
            if callbacks_container.get_callback(id).cb_type == CallbackType::Server {
                return false;
            }
            for next_id in callback_to_edges.get(id).unwrap().iter() {
                if !callbacks_in_plan.contains(next_id) {
                    continue;
                }
                if !is_client_only(
                    next_id,
                    callbacks_container,
                    callbacks_in_plan,
                    callback_to_edges,
                    visited,
                ) {
                    visited.insert(*id, false);
                    return false;
                }
            }

            visited.insert(*id, true);
            return true;
        }

        enum PlanAssignment {
            ClientPre,
            Server,
            ClientPost,
        }

        fn get_cb_plan_assignment<I, V>(
            id: &CallbackId,
            callbacks_container: &CallbacksContainer<I, V>,
            callbacks_in_plan: &HashSet<CallbackId>,
            callbacks_to_ancestors: &HashMap<CallbackId, Vec<CallbackId>>,
            callbacks_to_dependents: &HashMap<CallbackId, Vec<CallbackId>>,
            visited_pre: &mut HashMap<CallbackId, bool>,
            visited_post: &mut HashMap<CallbackId, bool>,
        ) -> PlanAssignment {
            if callbacks_container.get_callback(id).cb_type == CallbackType::Server {
                return PlanAssignment::Server;
            }

            if let Some(is_pre) = visited_pre.get(id) {
                if *is_pre {
                    return PlanAssignment::ClientPre;
                }
            } else {
                if is_client_only(
                    id,
                    callbacks_container,
                    callbacks_in_plan,
                    callbacks_to_ancestors,
                    visited_pre,
                ) {
                    return PlanAssignment::ClientPre;
                }
            }

            if let Some(is_post) = visited_post.get(id) {
                if *is_post {
                    return PlanAssignment::ClientPost;
                }
            } else {
                if is_client_only(
                    id,
                    callbacks_container,
                    callbacks_in_plan,
                    callbacks_to_dependents,
                    visited_post,
                ) {
                    return PlanAssignment::ClientPost;
                }
            }

            return PlanAssignment::Server;
        }

        let mut execution_plan = ExecutionPlan::new();
        let callbacks_in_plans: HashSet<CallbackId> = HashSet::from_iter(full_plan.iter().cloned());
        let mut visited_pre: HashMap<CallbackId, bool> = HashMap::new();
        let mut visited_post: HashMap<CallbackId, bool> = HashMap::new();

        for id in full_plan.iter() {
            match get_cb_plan_assignment(
                id,
                &self.callbacks_container,
                &callbacks_in_plans,
                &self.callback_to_ancestors,
                &self.callback_to_dependents,
                &mut visited_pre,
                &mut visited_post,
            ) {
                PlanAssignment::ClientPre => {
                    execution_plan.client_pre_plan.push(*id);
                }
                PlanAssignment::Server => {
                    execution_plan.server_plan.push(*id);
                }
                PlanAssignment::ClientPost => {
                    execution_plan.client_post_plan.push(*id);
                }
            }
        }

        return execution_plan;
    }

    pub fn get_required_args(&self, plan: &Vec<CallbackId>) -> HashSet<I> {
        let mut args = HashSet::new();
        // TODO: Explore the idea of handling intermediary args are produced as outputs during
        // the execution, that maybe are not required. This is currently tricky to handle because
        // producing outputs is optional (and when an output is left in NoChange state, we need it
        // to be included as arg).
        for id in plan.iter() {
            let callback = &self.callbacks_container.get_callback(id);
            for input in callback.inputs.iter() {
                args.insert(*input);
            }
        }
        return args;
    }

    pub fn get_required_state(
        &self,
        updated_inputs: &Vec<I>,
        execution_plan: &Vec<CallbackId>,
    ) -> HashSet<I> {
        let mut available_inputs: HashSet<I> = HashSet::from_iter(updated_inputs.iter().cloned());
        let mut required_state: HashSet<I> = HashSet::new();

        for id in execution_plan.iter() {
            let callback = &self.callbacks_container.get_callback(id);
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
        for cb in self.callbacks_container.all_callbacks().iter() {
            for input in cb.callback.inputs.iter() {
                required_inputs.insert(input.clone());
            }
        }

        for cb in self.callbacks_container.all_callbacks().iter() {
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

    pub fn execute_plan(
        &self,
        execution_args: &Vec<ExecutionArg<V>>,
        plan: &Vec<CallbackId>,
    ) -> Vec<ExecutionArg<V>> {
        let mut state: HashMap<I, V> = HashMap::new();

        for arg in execution_args.iter() {
            let identifier = arg.value.to_identifier();
            state.insert(identifier, arg.value.clone());
        }

        let mut output_updates: Vec<V> = Vec::new();
        for id in plan.iter() {
            let callback = &self.callbacks_container.get_callback(id);

            let mut new_updates = (callback.cb.unwrap())(&state);
            for value in new_updates.iter() {
                let identifier = value.to_identifier();
                state.insert(identifier, value.clone());
            }
            output_updates.append(&mut new_updates);
        }

        println!("output_updates: {:?}", output_updates);
        return output_updates
            .iter()
            .cloned()
            .map(|value| ExecutionArg {
                value: value,
                state: ArgState::Updated,
            })
            .collect();
    }
}
