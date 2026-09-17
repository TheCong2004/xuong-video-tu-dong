import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faQuestion } from "@fortawesome/pro-solid-svg-icons";
import toast from "react-hot-toast";
import { useCapCutMate } from "../api/CapCutMateContext";
import * as api from "../api/capcutBeClient";
import * as local from "../api/capcutLocalClient";

export function HelpFab() {
  const mate = useCapCutMate();

  const onHelp = async () => {
    try {
      const online = await api.pingBackend();
      let doctorNote = "";
      try {
        const d = (await local.localDoctor()) as {
          ok?: boolean;
          platform?: string;
        };
        doctorNote = d.ok
          ? ` · doctor OK (${d.platform || "env"})`
          : " · doctor có cảnh báo";
      } catch {
        doctorNote = " · doctor lỗi";
      }
      toast(
        online
          ? `BE đang hoạt động ${mate.baseUrl}${doctorNote}. Draft Mate: thanh trên. Draft local: bảng bên phải / menu Draft local. Tài liệu: :30000/docs`
          : `BE ngoại tuyến — chạy capcut-mate (uv run main.py) tại ${mate.baseUrl}`,
        { duration: 6000 },
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Không kiểm tra được BE");
    }
  };

  return (
    <button
      type="button"
      className="fixed right-5 bottom-5 z-10 flex h-10 w-10 items-center justify-center rounded-full border border-white/10 bg-[#252830] text-white/70 shadow-lg hover:bg-[#2f333c]"
      title="Help — kiểm tra BE thật"
      onClick={() => void onHelp()}
    >
      <FontAwesomeIcon icon={faQuestion} />
    </button>
  );
}
