//! Server dialogs (`net.minecraft.server.dialog`): what `Dialog.DIRECT_CODEC`
//! decodes from a `show_dialog` packet, the values its inputs hold, and the
//! click event an action makes of them (`Action.createAction`).
use crate::text::Text;
use horizon_world::nbt::Tag;

/// `CommonDialogData` and the type's own fields.
#[derive(Clone, Debug)]
pub struct Dialog {
    pub kind: Kind,
    pub title: Text,
    pub external_title: Option<Text>,
    pub can_close_with_escape: bool,
    pub pause: bool,
    pub after_action: AfterAction,
    pub body: Vec<Body>,
    pub inputs: Vec<Input>,
}

/// The dialog types (`DialogTypes`).
#[derive(Clone, Debug)]
pub enum Kind {
    /// `NoticeDialog`: one button, "OK" when the server names none.
    Notice(Button),
    Confirmation {
        yes: Button,
        no: Button,
    },
    MultiAction {
        actions: Vec<Button>,
        exit: Option<Button>,
        columns: i32,
    },
    ServerLinks {
        exit: Option<Button>,
        columns: i32,
        button_width: i32,
    },
    /// `DialogListDialog`: its dialogs as sent (ids, a tag or inline dialogs).
    DialogList {
        dialogs: Tag,
        exit: Option<Button>,
        columns: i32,
        button_width: i32,
    },
}

/// `DialogAction`: the screen after an action runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterAction {
    Close,
    None,
    WaitForResponse,
}

/// `ActionButton`: `CommonButtonData` and what it does.
#[derive(Clone, Debug)]
pub struct Button {
    pub label: Text,
    pub tooltip: Option<Text>,
    pub width: i32,
    pub action: Option<Action>,
}

/// `Action`: a fixed click event, or one made from the inputs' values.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// `StaticAction`: a click event as it is.
    Static(Click),
    /// `CommandTemplate`: a command with `$(key)` replaced by input values.
    RunCommand(String),
    /// `CustomAll`: a custom click event whose payload is `additions` plus
    /// every input's value under its key.
    Custom { id: String, additions: Option<Tag> },
}

/// `ClickEvent`, the kinds a server may send (`isAllowedFromServer`).
#[derive(Clone, Debug, PartialEq)]
pub enum Click {
    OpenUrl(String),
    RunCommand(String),
    SuggestCommand(String),
    /// The dialog: an id of the `minecraft:dialog` registry or the dialog itself.
    ShowDialog(Tag),
    ChangePage(i32),
    CopyToClipboard(String),
    Custom {
        id: String,
        payload: Option<Tag>,
    },
}

/// `DialogBody`.
#[derive(Clone, Debug)]
pub enum Body {
    /// `PlainMessage`: centred text wrapped to `width`.
    Message { contents: Text, width: i32 },
    /// `ItemBody`: the item as `ItemStackTemplate` sent it.
    Item {
        item: Tag,
        description: Option<(Text, i32)>,
        show_decorations: bool,
        show_tooltip: bool,
        width: i32,
        height: i32,
    },
}

/// `Input`: the key its value goes by, and its control.
#[derive(Clone, Debug)]
pub struct Input {
    pub key: String,
    pub control: Control,
}

/// `InputControl`.
#[derive(Clone, Debug)]
pub enum Control {
    Text {
        width: i32,
        label: Text,
        label_visible: bool,
        initial: String,
        max_length: i32,
        /// `MultilineOptions`: the most lines and the height, when set.
        multiline: Option<(Option<i32>, Option<i32>)>,
    },
    Boolean {
        label: Text,
        initial: bool,
        on_true: String,
        on_false: String,
    },
    SingleOption {
        width: i32,
        /// Each option's id, its label and whether it is the initial one.
        options: Vec<(String, Option<Text>, bool)>,
        label: Text,
        label_visible: bool,
    },
    NumberRange {
        width: i32,
        label: Text,
        label_format: String,
        start: f32,
        end: f32,
        initial: Option<f32>,
        step: Option<f32>,
    },
}

/// What an input holds while the dialog is open.
#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    Text(String),
    Bool(bool),
    /// The chosen option's index.
    Option(usize),
    /// The slider's position, 0 to 1.
    Slider(f32),
}
