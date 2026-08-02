import {
  type MutableRefObject,
  type ReactNode,
  useLayoutEffect,
  useRef,
} from "react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

interface BackgroundState {
  element: HTMLElement;
  inert: boolean;
  ariaHidden: string | null;
}

interface ModalEntry {
  container: HTMLElement;
  initialFocus: HTMLElement | null;
  restoreFocus: HTMLElement | null;
  onEscape: () => void;
  background: BackgroundState[];
}

const modalStack: ModalEntry[] = [];

function isFocusable(element: HTMLElement): boolean {
  return (
    !element.hasAttribute("disabled") &&
    element.getAttribute("aria-hidden") !== "true" &&
    element.getAttribute("tabindex") !== "-1" &&
    !element.hidden
  );
}

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    isFocusable,
  );
}

function focusInitial(entry: ModalEntry) {
  const target =
    entry.initialFocus && entry.container.contains(entry.initialFocus)
      ? entry.initialFocus
      : entry.container.querySelector<HTMLElement>("[data-modal-initial-focus]") ??
        getFocusableElements(entry.container)[0] ??
        entry.container;
  target.focus();
}

function hideBackground(container: HTMLElement): BackgroundState[] {
  const changed: BackgroundState[] = [];
  let branch: HTMLElement | null = container;

  while (branch?.parentElement) {
    const parentElement: HTMLElement = branch.parentElement;
    for (const sibling of Array.from(parentElement.children)) {
      if (!(sibling instanceof HTMLElement) || sibling === branch) continue;
      changed.push({
        element: sibling,
        inert: sibling.inert,
        ariaHidden: sibling.getAttribute("aria-hidden"),
      });
      sibling.inert = true;
      sibling.setAttribute("aria-hidden", "true");
    }
    branch = parentElement;
    if (parentElement === document.body) break;
  }

  return changed;
}

function restoreBackground(states: BackgroundState[]) {
  for (const { element, inert, ariaHidden } of states.reverse()) {
    element.inert = inert;
    if (ariaHidden === null) element.removeAttribute("aria-hidden");
    else element.setAttribute("aria-hidden", ariaHidden);
  }
}

function handleModalKeyDown(event: KeyboardEvent) {
  const entry = modalStack[modalStack.length - 1];
  if (!entry) return;

  if (event.key === "Escape") {
    event.preventDefault();
    event.stopImmediatePropagation();
    entry.onEscape();
    return;
  }

  if (event.key !== "Tab") return;

  const focusable = getFocusableElements(entry.container);
  if (focusable.length === 0) {
    event.preventDefault();
    entry.container.focus();
    return;
  }

  const active = document.activeElement as HTMLElement | null;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const outside = !active || !entry.container.contains(active);

  if (outside || (!event.shiftKey && active === last)) {
    event.preventDefault();
    first.focus();
  } else if (event.shiftKey && active === first) {
    event.preventDefault();
    last.focus();
  }
}

function registerModal(entry: ModalEntry) {
  if (modalStack.length === 0) {
    document.addEventListener("keydown", handleModalKeyDown, true);
  }
  entry.background = hideBackground(entry.container);
  modalStack.push(entry);
  focusInitial(entry);
}

function unregisterModal(entry: ModalEntry) {
  const index = modalStack.lastIndexOf(entry);
  if (index === -1) return;
  const wasTopmost = index === modalStack.length - 1;
  modalStack.splice(index, 1);
  restoreBackground(entry.background);

  if (modalStack.length === 0) {
    document.removeEventListener("keydown", handleModalKeyDown, true);
  }

  if (!wasTopmost) return;
  const nextModal = modalStack[modalStack.length - 1];
  const restoreTarget = entry.restoreFocus;
  if (
    restoreTarget?.isConnected &&
    !restoreTarget.inert &&
    restoreTarget.getAttribute("aria-hidden") !== "true"
  ) {
    restoreTarget.focus();
  } else if (nextModal) {
    focusInitial(nextModal);
  }
}

export interface ModalSurfaceProps {
  children: ReactNode;
  className: string;
  role?: "dialog" | "alertdialog";
  labelledBy: string;
  describedBy?: string;
  onEscape: () => void;
  initialFocusRef?: MutableRefObject<HTMLElement | null>;
}

export function ModalSurface({
  children,
  className,
  role = "dialog",
  labelledBy,
  describedBy,
  onEscape,
  initialFocusRef,
}: ModalSurfaceProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const onEscapeRef = useRef(onEscape);
  onEscapeRef.current = onEscape;

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const entry: ModalEntry = {
      container,
      initialFocus: initialFocusRef?.current ?? null,
      restoreFocus:
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null,
      onEscape: () => onEscapeRef.current(),
      background: [],
    };
    registerModal(entry);
    return () => unregisterModal(entry);
  }, [initialFocusRef]);

  return (
    <div
      ref={containerRef}
      className={className}
      role={role}
      aria-modal="true"
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      tabIndex={-1}
    >
      {children}
    </div>
  );
}
