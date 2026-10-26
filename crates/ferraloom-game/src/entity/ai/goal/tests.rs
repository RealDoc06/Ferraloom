use super::*;
use std::collections::BTreeMap;
const VECTORS: &str = include_str!("../../../../tests/java/ai/goal/vectors.tsv");
#[derive(Default)]
struct Probe {
    flags: Flags,
    usable: bool,
    continuing: bool,
    interruptable: bool,
    every: bool,
}
#[derive(Default)]
struct Fixture {
    selector: GoalSelector,
    goals: Vec<Probe>,
    names: BTreeMap<String, GoalId>,
    calls: Vec<String>,
}
impl Fixture {
    fn id(&mut self, name: &str) -> GoalId {
        if let Some(id) = self.names.get(name) {
            return *id;
        }
        let id = GoalId(self.goals.len());
        self.goals.push(Probe {
            usable: true,
            continuing: true,
            interruptable: true,
            ..Default::default()
        });
        self.names.insert(name.into(), id);
        id
    }
    fn name(&self, id: GoalId) -> &str {
        self.names
            .iter()
            .find_map(|(name, &key)| (key == id).then_some(name.as_str()))
            .unwrap()
    }
    fn call(&mut self, id: GoalId, method: &str) {
        self.calls.push(format!("{}.{method}", self.name(id)));
    }
    fn apply(&mut self, input: &str) {
        let a: Vec<_> = input.split(',').collect();
        match a[0] {
            "add" => {
                let id = self.id(a[1]);
                self.goals[id.0].flags = flags(a[3]);
                self.selector.add_goal(a[2].parse().unwrap(), id);
            }
            "use" | "continue" | "interrupt" | "every" => {
                let id = self.id(a[1]);
                let value = a[2].parse().unwrap();
                match a[0] {
                    "use" => self.goals[id.0].usable = value,
                    "continue" => self.goals[id.0].continuing = value,
                    "interrupt" => self.goals[id.0].interruptable = value,
                    _ => self.goals[id.0].every = value,
                }
            }
            "flags" => {
                let id = self.id(a[1]);
                self.goals[id.0].flags = flags(a[2]);
            }
            "disable" | "enable" => self.selector.set_control_flag(flag(a[1]), a[0] == "enable"),
            "tick" => tick(self).unwrap(),
            "fast" => tick_running(self, false).unwrap(),
            "force" => tick_running(self, true).unwrap(),
            "remove" => {
                let target = self.id(a[1]);
                remove_all(self, |_, goal| Ok(goal == target)).unwrap();
            }
            "removeAll" => remove_all(self, |r, goal| {
                r.call(goal, "predicate");
                Ok(a[1].contains(r.name(goal)))
            })
            .unwrap(),
            _ => panic!("{input}"),
        }
    }
    fn snapshot(&self) -> String {
        let available = self
            .selector
            .available_goals()
            .map(|g| format!("{}:{}:{}", self.name(g.goal), g.priority, g.is_running()))
            .collect::<Vec<_>>()
            .join(",");
        let locks = Flag::ALL
            .into_iter()
            .filter_map(|f| {
                self.selector
                    .locked_goal(f)
                    .map(|g| format!("{}:{}", flag_name(f), self.name(g)))
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{available}/{locks}")
    }
}
impl GoalRuntime for Fixture {
    type Error = AiOperation;
    fn selector(&self) -> &GoalSelector {
        &self.selector
    }
    fn selector_mut(&mut self) -> &mut GoalSelector {
        &mut self.selector
    }
    fn flags(&self, id: GoalId) -> Flags {
        self.goals[id.0].flags
    }
    fn can_use(&mut self, id: GoalId) -> Result<bool, AiOperation> {
        self.call(id, "use");
        Ok(self.goals[id.0].usable)
    }
    fn can_continue_to_use(&mut self, id: GoalId) -> Result<bool, AiOperation> {
        self.call(id, "continue");
        Ok(self.goals[id.0].continuing)
    }
    fn is_interruptable(&mut self, id: GoalId) -> Result<bool, AiOperation> {
        self.call(id, "interrupt");
        Ok(self.goals[id.0].interruptable)
    }
    fn requires_update_every_tick(&mut self, id: GoalId) -> Result<bool, AiOperation> {
        self.call(id, "every");
        Ok(self.goals[id.0].every)
    }
    fn start(&mut self, id: GoalId) -> Result<(), AiOperation> {
        self.call(id, "start");
        Ok(())
    }
    fn stop(&mut self, id: GoalId) -> Result<(), AiOperation> {
        self.call(id, "stop");
        Ok(())
    }
    fn tick(&mut self, id: GoalId) -> Result<(), AiOperation> {
        self.call(id, "tick");
        Ok(())
    }
}
fn flag(value: &str) -> Flag {
    match value {
        "MOVE" => Flag::Move,
        "LOOK" => Flag::Look,
        "JUMP" => Flag::Jump,
        "TARGET" => Flag::Target,
        _ => panic!("{value}"),
    }
}
fn flag_name(value: Flag) -> &'static str {
    match value {
        Flag::Move => "MOVE",
        Flag::Look => "LOOK",
        Flag::Jump => "JUMP",
        Flag::Target => "TARGET",
    }
}
fn flags(value: &str) -> Flags {
    let mut flags = Flags::default();
    if value != "-" {
        for value in value.split('+') {
            flags.insert(flag(value));
        }
    }
    flags
}
#[test]
fn native_selector_priority_flags_identity_removal_and_reduced_cadence() {
    let mut current = "";
    let mut fixture = Fixture::default();
    let mut checked = 0;
    for row in VECTORS.lines().filter(|r| r.starts_with("selector\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[1] != current {
            fixture = Fixture::default();
            current = fields[1];
        }
        fixture.calls.clear();
        fixture.apply(fields[2]);
        let calls = if fixture.calls.is_empty() {
            "-".into()
        } else {
            fixture.calls.join("|")
        };
        assert_eq!(calls, fields[3], "{row}");
        assert_eq!(fixture.snapshot(), fields[4], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 63);
}
#[test]
fn native_goal_tick_delay_includes_negative_and_wrapped_integer_boundaries() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|r| r.starts_with("delay\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(
            adjusted_tick_delay(fields[2].parse().unwrap(), fields[1].parse().unwrap()),
            fields[3].parse::<i32>().unwrap(),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 20);
}
