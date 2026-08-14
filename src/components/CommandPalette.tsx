import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ModalSurface } from "./ModalSurface";

export interface CommandDefinition {
  id: string;
  label: string;
  keywords: string[];
  shortcut?: string;
  disabled?: boolean;
  execute: () => void | Promise<void | boolean>;
}

interface CommandPaletteProps {
  open: boolean;
  commands: CommandDefinition[];
  onClose: () => void;
}

export function CommandPalette({ open, commands, onClose }: CommandPaletteProps) {
  const { t } = useTranslation();
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [running, setRunning] = useState(false);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return commands;
    return commands.filter((command) =>
      [command.label, ...command.keywords]
        .join(" ")
        .toLocaleLowerCase()
        .includes(normalized),
    );
  }, [commands, query]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    setRunning(false);
  }, [open]);

  useEffect(() => {
    setActiveIndex((current) => Math.min(current, Math.max(filtered.length - 1, 0)));
  }, [filtered.length]);

  if (!open) return null;

  const execute = async (command: CommandDefinition | undefined) => {
    if (!command || command.disabled || running) return;
    setRunning(true);
    onClose();
    try {
      await command.execute();
    } finally {
      setRunning(false);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (filtered.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((current) => (current + 1) % filtered.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) => (current - 1 + filtered.length) % filtered.length);
    } else if (event.key === "Home") {
      event.preventDefault();
      setActiveIndex(0);
    } else if (event.key === "End") {
      event.preventDefault();
      setActiveIndex(filtered.length - 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void execute(filtered[activeIndex]);
    }
  };

  const activeCommand = filtered[activeIndex];

  return (
    <div className="command-palette-overlay">
      <ModalSurface
        className="command-palette"
        labelledBy="command-palette-title"
        describedBy="command-palette-description"
        initialFocusRef={inputRef}
        onEscape={onClose}
      >
        <div className="command-palette-heading">
          <h2 id="command-palette-title">{t("commandPalette.title")}</h2>
          <span className="command-palette-hint" aria-hidden="true">Esc</span>
        </div>
        <p id="command-palette-description" className="sr-only">
          {t("commandPalette.description")}
        </p>
        <input
          ref={inputRef}
          className="command-palette-input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t("commandPalette.placeholder")}
          aria-label={t("commandPalette.placeholder")}
          aria-controls="command-palette-list"
          aria-activedescendant={activeCommand ? `command-${activeCommand.id}` : undefined}
          autoComplete="off"
        />
        {filtered.length === 0 ? (
          <p className="command-palette-empty" role="status">{t("commandPalette.empty")}</p>
        ) : (
          <div id="command-palette-list" className="command-palette-list" role="listbox">
            {filtered.map((command, index) => {
              const active = index === activeIndex;
              return (
                <button
                  id={`command-${command.id}`}
                  key={command.id}
                  type="button"
                  role="option"
                  className={`command-palette-item ${active ? "active" : ""}`}
                  aria-selected={active}
                  aria-disabled={command.disabled || running}
                  disabled={command.disabled || running}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => void execute(command)}
                >
                  <span>{command.label}</span>
                  {command.shortcut && <kbd>{command.shortcut}</kbd>}
                </button>
              );
            })}
          </div>
        )}
      </ModalSurface>
    </div>
  );
}
