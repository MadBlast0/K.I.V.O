/**
 * The licence, attribution, source and size of a model before it downloads (DIST-13, VOICE-45):
 * nothing downloads until the user has seen them. Voice → Models on this PC and Settings →
 * Performance's GPU acceleration for NVIDIA (VOICE-50) both ask through it.
 */
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { ModelItem } from "../../ipc/generated";
import { Button, Dialog, DialogClose } from "../ui";

/** Megabytes in the user's language, at least 1. */
export function useMegabytes(): (bytes: number) => string {
  const { i18n } = useTranslation();
  const format = useMemo(
    () => new Intl.NumberFormat(i18n.language, { style: "unit", unit: "megabyte", maximumFractionDigits: 0 }),
    [i18n.language],
  );
  return (bytes: number) => format.format(Math.max(1, Math.round(bytes / 1_000_000)));
}

export function DownloadDialog({
  model,
  onClose,
  onDownload,
}: {
  model: ModelItem | null;
  onClose: () => void;
  onDownload: (model: ModelItem) => void;
}) {
  const { t } = useTranslation();
  const size = useMegabytes();
  return (
    <Dialog
      open={model !== null}
      onOpenChange={(open) => !open && onClose()}
      title={model ? t("voice.downloadTitle", { name: model.name }) : ""}
      description={model ? t("voice.downloadSize", { size: size(model.size) }) : ""}
      footer={
        <>
          <DialogClose>
            <Button>{t("voice.cancel")}</Button>
          </DialogClose>
          <Button variant="primary" icon="download" onClick={() => model && onDownload(model)}>
            {t("voice.download")}
          </Button>
        </>
      }
    >
      {model && (
        <div className="k-licence">
          <p>
            <b>{t("voice.license")}</b> {model.license}
          </p>
          <p>{model.attribution}</p>
          <p className="k-licence__source">{model.source}</p>
        </div>
      )}
    </Dialog>
  );
}
