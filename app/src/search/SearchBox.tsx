import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { commands } from "../bindings";
import { queryKeys, SUGGESTIONS_STALE_MS } from "../query";
import { useDebouncedValue } from "./useDebouncedValue";

/** Delay between the last keystroke and the suggestions request. */
export const SUGGESTION_DELAY_MS = 250;

type Props = {
  initialText: string;
  onSubmit: (query: string) => void;
  /** Focuses the field on mount and whenever this number changes. */
  focusRequest: number;
};

/** A search field with autocomplete (ARIA combobox with a listbox popup). */
export function SearchBox({ initialText, onSubmit, focusRequest }: Props) {
  const [text, setText] = useState(initialText);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  const debouncedText = useDebouncedValue(text.trim(), SUGGESTION_DELAY_MS);

  const suggestions = useQuery({
    queryKey: queryKeys.suggestions(debouncedText),
    queryFn: () => commands.searchSuggestions(debouncedText),
    enabled: open && debouncedText !== "",
    staleTime: SUGGESTIONS_STALE_MS,
    placeholderData: keepPreviousData,
  });
  // Suggestion failures are not worth an alert: the user can still press Return.
  const availableOptions = text.trim() !== "" ? (suggestions.data ?? []) : [];
  const options = open ? availableOptions : [];
  const activeIndex = active < options.length ? active : -1;

  useEffect(() => {
    input.current?.focus();
  }, [focusRequest]);

  function submit(value: string) {
    const query = value.trim();
    if (query === "") return;
    setText(query);
    setOpen(false);
    setActive(-1);
    onSubmit(query);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.nativeEvent.isComposing || event.keyCode === 229) return;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setOpen(true);
        setActive(Math.min(activeIndex + 1, availableOptions.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive(Math.max(activeIndex - 1, -1));
        break;
      case "Enter":
        if (activeIndex >= 0) {
          event.preventDefault();
          submit(options[activeIndex]);
        }
        break;
      case "Escape":
        if (options.length > 0) {
          event.preventDefault();
          setOpen(false);
          setActive(-1);
        }
        break;
    }
  }

  return (
    <form
      role="search"
      className="search-box"
      onSubmit={(event) => {
        event.preventDefault();
        submit(input.current?.value ?? text);
      }}
    >
      <input
        ref={input}
        type="search"
        role="combobox"
        aria-label="Search YouTube Music"
        aria-autocomplete="list"
        aria-expanded={options.length > 0}
        aria-controls={listId}
        aria-activedescendant={activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined}
        placeholder="Songs, albums, artists"
        autoComplete="off"
        spellCheck={false}
        maxLength={200}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setOpen(true);
          setActive(-1);
        }}
        onKeyDown={onKeyDown}
        onBlur={() => setOpen(false)}
      />
      <ul id={listId} role="listbox" aria-label="Suggestions" className="suggestions" hidden={options.length === 0}>
        {options.map((option, index) => (
          <li
            key={option}
            id={`${listId}-${index}`}
            role="option"
            aria-selected={index === activeIndex}
            // Keep focus in the field so the click lands before blur closes the list.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => submit(option)}
          >
            {option}
          </li>
        ))}
      </ul>
    </form>
  );
}
