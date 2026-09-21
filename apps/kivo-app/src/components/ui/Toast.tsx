/** In-app toasts: black capsule at the bottom centre, with an optional Undo. */
import { Toast as BToast } from "@base-ui/react/toast";
import type { ReactNode } from "react";
import { Icon } from "../../icons";

export function ToastProvider({ children }: { children: ReactNode }) {
  return (
    <BToast.Provider timeout={3200} limit={3}>
      {children}
      <ToastViewport />
    </BToast.Provider>
  );
}

function ToastViewport() {
  const { toasts } = BToast.useToastManager();
  return (
    <BToast.Portal>
      <BToast.Viewport className="k-toast-viewport">
        {toasts.map((t) => (
          <BToast.Root key={t.id} toast={t} className="k-toast">
            <Icon name="check" />
            <BToast.Title>{t.title}</BToast.Title>
            {t.actionProps && <BToast.Action className="k-btn" />}
          </BToast.Root>
        ))}
      </BToast.Viewport>
    </BToast.Portal>
  );
}

/** Show a toast. Pass `onUndo` to add an Undo button. */
export function useToast() {
  const manager = BToast.useToastManager();
  return (title: ReactNode, opts?: { onUndo?: () => void }) =>
    manager.add({ title, actionProps: opts?.onUndo ? { children: "Undo", onClick: opts.onUndo } : undefined });
}
