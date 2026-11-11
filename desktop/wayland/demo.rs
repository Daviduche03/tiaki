// Tiaki — the desktop is data.
use agentic_ui_tree::{UiOp, UiTree};

fn main() {
    let tree = UiTree::parse(include_str!("desktop.json")).unwrap();

    let ops = vec![
        UiOp::Move {
            id: "clock".into(),
            parent: "panel.main".into(),
            index: 0,
        },
        UiOp::SetProp {
            id: "clock".into(),
            key: "format".into(),
            value: "12h".into(),
        },
    ];

    let mut next = tree;
    agentic_ui_tree::apply_ops(&mut next, &ops).unwrap();
    println!("{}", next.render());
}
