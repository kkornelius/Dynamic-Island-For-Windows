use super::*;
pub(super) fn draw(v: &mut View<'_>, now: u64) {
    let n = &v.s.note;
    let active = v.s.editing == Some(EditField::Note);
    v.label("Quick notes", 28, 65, 250, 34, 23, true, WHITE);
    v.button(
        if n.copied_until > now {
            "Copied"
        } else {
            "Copy"
        },
        [346, 69, 82, 32],
        Action::NoteCopy,
        false,
        !n.editor.text.is_empty(),
    );
    let [x, y, w, h] = crate::notes::BODY;
    v.c.round(x, y, w, h, 17, 0x171717);
    v.c.outline(x, y, w, h, 17, if active { 0x666666 } else { 0x303030 }, 1.);
    if n.editor.text.is_empty() {
        v.label(
            if n.readable {
                "Write something to remember…"
            } else {
                "Your saved note couldn't be opened."
            },
            44,
            134,
            360,
            25,
            14,
            false,
            DIM,
        );
    }
    v.c.note_draw(
        &n.editor,
        active && (now.saturating_sub(n.caret_at) % 1000 < 500),
        active,
    );
    v.hits.push(Hit {
        rect: crate::notes::BODY,
        action: Action::NoteEdit,
    });
    let status = if !n.readable {
        "Saved note preserved. Check the data folder."
    } else if n.error {
        "Couldn't save. Retrying…"
    } else if n.editor.revision != n.saved {
        "Saving…"
    } else {
        "Saved on this PC"
    };
    v.label(status, 30, 389, 285, 23, 11, false, MUTED);
    v.c.text(
        &format!("{} / 20,000", n.editor.text.chars().count()),
        310,
        389,
        116,
        23,
        11,
        false,
        DIM,
        true,
    );
    v.label(
        "Enter for a new line · Ctrl+Enter to finish",
        30,
        413,
        398,
        20,
        11,
        false,
        DIM,
    );
}
