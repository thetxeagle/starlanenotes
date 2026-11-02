import { EmojiPicker as FrimousseEmojiPicker } from "frimousse";

interface EmojiPickerContentProps {
  onSelect: (emoji: string) => void;
}

export const STARLANE_EMOJI_DATA_URL = "/emoji-data";

/** Shared full emoji catalog used by both inline editing and page identity icons. */
export function EmojiPickerContent({ onSelect }: EmojiPickerContentProps) {
  return (
    <FrimousseEmojiPicker.Root
      className="starlane-emoji-picker-root"
      columns={8}
      emojibaseUrl={STARLANE_EMOJI_DATA_URL}
      sticky
      onEmojiSelect={({ emoji }) => onSelect(emoji)}
    >
      <div className="starlane-emoji-picker-heading">
        <div>
          <p className="text-text text-sm font-medium">Emoji</p>
          <p className="text-text-muted text-xs">Search or browse every emoji</p>
        </div>
        <FrimousseEmojiPicker.SkinToneSelector
          type="button"
          aria-label="Change emoji skin tone"
          className="starlane-emoji-picker-tone"
        />
      </div>
      <FrimousseEmojiPicker.Search
        placeholder="Search emoji"
        aria-label="Search emoji"
        className="starlane-emoji-picker-search"
      />
      <FrimousseEmojiPicker.Viewport className="starlane-emoji-picker-viewport">
        <FrimousseEmojiPicker.Loading>Loading emoji…</FrimousseEmojiPicker.Loading>
        <FrimousseEmojiPicker.Empty>No emoji found.</FrimousseEmojiPicker.Empty>
        <FrimousseEmojiPicker.List
          className="starlane-emoji-picker-list"
          components={{
            CategoryHeader: ({ category, ...props }) => (
              <div {...props} className="starlane-emoji-picker-category">
                {category.label}
              </div>
            ),
            Row: ({ children, ...props }) => (
              <div {...props} className="starlane-emoji-picker-row">
                {children}
              </div>
            ),
            Emoji: ({ emoji, ...props }) => (
              <button
                {...props}
                type="button"
                className="starlane-emoji-picker-emoji"
                aria-label={emoji.label}
              >
                {emoji.emoji}
              </button>
            ),
          }}
        />
      </FrimousseEmojiPicker.Viewport>
      <div className="starlane-emoji-picker-footer">
        <FrimousseEmojiPicker.ActiveEmoji>
          {({ emoji }) => emoji?.label ?? "Choose an emoji"}
        </FrimousseEmojiPicker.ActiveEmoji>
      </div>
    </FrimousseEmojiPicker.Root>
  );
}
