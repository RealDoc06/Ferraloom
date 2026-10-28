use super::*;
use ferraloom_data::Item;
use serde::Deserialize;
use std::{cell::RefCell, convert::Infallible, rc::Rc};

#[derive(Deserialize)]
struct Vector {
    action: String,
    size: usize,
    selected: i32,
    updated: i32,
    #[serde(rename = "final")]
    final_counts: Vec<i32>,
    copies: Option<Vec<i32>>,
    trace: Vec<String>,
}
struct Operation {
    values: Vec<ItemStack>,
    trace: Rc<RefCell<Vec<String>>>,
}
fn count(value: &ItemStack) -> i32 {
    if value.is_empty() {
        0
    } else {
        value.0.as_ref().unwrap().count
    }
}
fn stack(count: i32) -> ItemStack {
    ItemStack::new(Item::STONE, count, Default::default())
}
impl SlotOperation<usize> for Operation {
    type Error = Infallible;
    fn get(&mut self, index: &usize) -> Result<ItemStack, Infallible> {
        let value = &self.values[*index];
        self.trace
            .borrow_mut()
            .push(format!("get:{index}:{}", count(value)));
        Ok(value.clone())
    }
    fn set(&mut self, index: &usize, value: ItemStack) -> Result<bool, Infallible> {
        self.trace
            .borrow_mut()
            .push(format!("set:{index}:{}", count(&value)));
        if *index == 1 {
            return Ok(false);
        }
        self.values[*index] = value;
        Ok(true)
    }
}
#[test]
fn native_slot_collection_reads_copies_rejections_and_counting_modifier() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../../tests/java/commands/item/vectors.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 8);
    for vector in vectors {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut operation = Operation {
            values: vec![stack(2), stack(3), stack(0)],
            trace: trace.clone(),
        };
        let slots = SlotCollection::new(vec![0, 1, 0, 2]);
        assert_eq!(slots.size(), vector.size);
        let result = if vector.action == "copies" {
            let values = slots.item_copies(&mut operation).unwrap();
            assert_eq!(
                Some(values.iter().map(count).collect::<Vec<_>>()),
                vector.copies
            );
            SlotUpdate::default()
        } else if let Some(count) = vector.action.strip_prefix("replace") {
            let items: Vec<_> = (0..count.parse::<i32>().unwrap())
                .map(|i| stack(10 + i))
                .collect();
            slots.replace(&items, &mut operation).unwrap()
        } else {
            slots
                .modify(&mut operation, |value| {
                    trace.borrow_mut().push(format!("modify:{}", count(&value)));
                    Ok(match vector.action.as_str() {
                        "clear" => stack(0),
                        "double" => stack(count(&value) * 2),
                        _ => value,
                    })
                })
                .unwrap()
        };
        assert_eq!(
            result,
            SlotUpdate {
                selected: vector.selected,
                updated: vector.updated
            },
            "{}",
            vector.action
        );
        assert_eq!(
            operation.values.iter().map(count).collect::<Vec<_>>(),
            vector.final_counts,
            "{}",
            vector.action
        );
        assert_eq!(*trace.borrow(), vector.trace, "{}", vector.action);
    }
}
