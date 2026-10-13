import { useState } from "react";
import { AlertDialog, Button } from "@heroui/react";
import { LuCpu } from "react-icons/lu";
import { useLogicalNavigation } from "./logical-navigation";
import type { OcrModelStatus } from "./OcrModelSettings";

export function OcrModelRequiredDialog({
  status,
  isOpen,
  onOpenChange,
}: {
  status?: OcrModelStatus;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigation = useLogicalNavigation();
  const [opening, setOpening] = useState(false);
  return (
    <AlertDialog isOpen={isOpen} onOpenChange={(open) => open && onOpenChange(true)}>
      <AlertDialog.Backdrop isDismissable={false} isKeyboardDismissDisabled>
        <AlertDialog.Container size="sm">
          <AlertDialog.Dialog aria-labelledby="ocr-required-title">
            <AlertDialog.Header>
              <AlertDialog.Icon status="warning">
                <LuCpu aria-hidden="true" />
              </AlertDialog.Icon>
              <AlertDialog.Heading id="ocr-required-title">
                需要安装本地 OCR 模型
              </AlertDialog.Heading>
            </AlertDialog.Header>
            <AlertDialog.Body>
              当前验证码模块需要本地模型才能继续。请前往设置完成下载或更新，模型只保存在本机。
              {status?.error && <p className="mt-2 text-sm text-danger">{status.error}</p>}
            </AlertDialog.Body>
            <AlertDialog.Footer>
              <Button
                isPending={opening}
                onPress={async () => {
                  setOpening(true);
                  try {
                    await navigation.push("/settings?focus=ocr");
                  } finally {
                    setOpening(false);
                  }
                }}
              >
                前往设置下载模型
              </Button>
            </AlertDialog.Footer>
          </AlertDialog.Dialog>
        </AlertDialog.Container>
      </AlertDialog.Backdrop>
    </AlertDialog>
  );
}
