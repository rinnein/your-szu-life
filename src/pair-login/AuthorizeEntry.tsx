import { useState } from "react";
import { createPortal } from "react-dom";
import { Button, Card, Dialog, TextField } from "material/react";
import { SettingList, SettingRow, bindField, toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { useAuth } from "../auth";
import { useLogicalNavigation } from "../logical-navigation";
import { useAndroid } from "../use-android";
import { scanQrOnce } from "../qr-scan";
import { pairLoginAcceptUrl } from "./api";

/**
 * 设置-个人信息里的「授权其他设备登录」入口。
 *
 * 深链在部分机型上会被系统相机/浏览器拦下，这里提供应用内扫码（Android）
 * 与粘贴链接两种兜底；两者最终都走同一条 Rust 侧 accept → 授权页流程。
 */
export function PairAuthorizeEntry() {
  const { state } = useAuth();
  const signedIn = "Authenticated" in state;
  const navigation = useLogicalNavigation();
  const platform = useAndroid();
  const [open, setOpen] = useState(false);
  const [link, setLink] = useState("");
  const [error, setError] = useState<string | null>(null);
  const { run, isPending } = usePending();
  const busy = isPending("accept");

  const close = () => {
    if (busy) return;
    setOpen(false);
    setError(null);
  };

  const submit = async (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) {
      setError("请粘贴配对链接");
      return;
    }
    const result = await run("accept", () => pairLoginAcceptUrl(trimmed));
    if (!result.ok) {
      if (!result.skipped) setError(errorMessage(result.error));
      return;
    }
    setError(null);
    setOpen(false);
    await navigation.push("/pair-login");
  };

  const scan = async () => {
    try {
      const content = await scanQrOnce();
      setLink(content);
      await submit(content);
    } catch (cause) {
      // 用户取消取景器是正常路径，不当作失败提示。
      const message = errorMessage(cause, "");
      if (message) toast.warning("无法扫码", { description: message });
    }
  };

  return (
    <>
      <Card variant="outlined">
        <div slot="headline">授权其他设备登录</div>
        <div slot="subhead">
          为一台正在初始化的新设备完成统一认证。需要新设备先在登录步骤打开「远程授权登录」。
        </div>
        <SettingList>
          <SettingRow
            title="扫码或粘贴配对链接"
            description={
              signedIn
                ? "会先显示新设备信息与 6 位配对码，确认后才发送本机保存的校园卡号和密码。"
                : "需要先在本机完成统一认证。"
            }
          >
            <Button variant="tonal" disabled={!signedIn} onClick={() => setOpen(true)}>
              开始授权
            </Button>
          </SettingRow>
        </SettingList>
      </Card>

      {open &&
        createPortal(
          <Dialog
            open
            onClose={close}
            headline="授权其他设备登录"
            size="small"
            actions={
              <>
                <Button variant="text" disabled={busy} onClick={close}>
                  取消
                </Button>
                <Button variant="filled" disabled={busy} onClick={() => void submit(link)}>
                  继续
                </Button>
              </>
            }
          >
            <div className="grid gap-3">
              <p className="text-sm text-muted">
                在新设备的「账户与个人信息」里点击「远程授权登录」，然后扫描它显示的二维码，或粘贴完整配对链接。
              </p>
              <TextField
                label="配对链接"
                value={link}
                error={error !== null}
                supportingText={error ?? "以 your-szu-life://page/pair-login 开头"}
                {...bindField(setLink)}
              />
              {platform === "android" && (
                <div>
                  <Button variant="tonal" disabled={busy} onClick={() => void scan()}>
                    扫描二维码
                  </Button>
                </div>
              )}
            </div>
          </Dialog>,
          document.body,
        )}
    </>
  );
}
