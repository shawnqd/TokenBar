import type { FontInstallGuide } from "../../lib/fontWhitelist";

export default function FontInstallDialog({
  guide,
  miss,
  onCancel,
  onOpenDownload,
  onRecheck,
}: {
  guide: FontInstallGuide;
  miss: boolean;
  onCancel: () => void;
  onOpenDownload: () => void;
  onRecheck: () => void;
}) {
  return (
    <div className="s5-modal-back s5-modal-back--fixed" data-settings-dialog>
      <div className="s5-modal" role="dialog" aria-modal="true" aria-labelledby="font-install-title">
        <h4 id="font-install-title">安装 {guide.label}</h4>
        <p>
          应用只内置 MiSans VF。请打开官方页下载可变字体
          <code> {guide.file} </code>
          ，右键安装到 Windows，然后点「我已安装」。
        </p>
        {miss ? (
          <p>还没检测到这款字体。确认装的是可变字体，必要时关掉再开设置窗。</p>
        ) : null}
        <div className="s5-modal-actions">
          <button type="button" className="s5-ghost" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="s5-ghost" onClick={onOpenDownload}>
            打开下载页
          </button>
          <button type="button" className="s5-ghost" onClick={onRecheck}>
            我已安装
          </button>
        </div>
      </div>
    </div>
  );
}
