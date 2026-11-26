import { useEffect, useRef, type KeyboardEvent, type ReactElement, type RefObject } from "react";

type RenameInputProps = {
  initial: string;
  pending: boolean;
  onConfirm: (name: string) => void;
  onCancel: () => void;
};

export function RenameInput(props: RenameInputProps): ReactElement {
  const ref: RefObject<HTMLInputElement | null> = useRef<HTMLInputElement | null>(null);

  useEffect((): void => {
    const input: HTMLInputElement | null = ref.current;
    if (input !== null) {
      input.focus();
      const dot: number = props.initial.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : props.initial.length);
    }
  }, [props.initial]);

  const handleKey = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === "Escape") {
      event.stopPropagation();
      if (!props.pending) {
        props.onCancel();
      }
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const value: string = event.currentTarget.value.trim();
      if (value === "" || value === props.initial) {
        props.onCancel();
        return;
      }
      props.onConfirm(value);
    }
  };

  return (
    <input
      ref={ref}
      type="text"
      className="rename-input"
      defaultValue={props.initial}
      disabled={props.pending}
      aria-label="New file name"
      onKeyDown={handleKey}
      onBlur={(): void => {
        if (!props.pending) {
          props.onCancel();
        }
      }}
    />
  );
}
