import type { StorageConfig } from "./api-types";
import { field, h } from "./dom";

type Kind = "local" | "s3" | "r2" | "b2" | "gdrive";

/** Form for choosing and configuring the attachment backend (first-run setup and settings). */
export function storageForm(initial?: StorageConfig) {
  const kinds: [Kind, string][] = [
    ["local", "Local disk (testing / small installs)"],
    ["s3", "Amazon S3 or S3-compatible (MinIO)"],
    ["r2", "Cloudflare R2"],
    ["b2", "Backblaze B2"],
    ["gdrive", "Google Drive"],
  ];
  const cur: Kind = !initial ? "local" : initial.backend === "s3" ? (initial.provider === "r2" ? "r2" : initial.provider === "b2" ? "b2" : "s3") : (initial.backend as Kind);
  const sel = h("select", {}, ...kinds.map(([k, l]) => h("option", { value: k, selected: k === cur }, l)));
  const box = h("div", {});
  let inputs: Record<string, HTMLInputElement> = {};
  const inp = (key: string, label: string, opts: { type?: string; placeholder?: string; value?: string } = {}) => {
    const i = h("input", { type: opts.type ?? "text", placeholder: opts.placeholder ?? "", value: opts.value ?? "", autocomplete: "off" });
    inputs[key] = i;
    return field(label, i);
  };
  const s3 = initial?.backend === "s3" ? initial : undefined;
  const gd = initial?.backend === "gdrive" ? initial : undefined;
  const check = (key: string, label: string, on: boolean) => {
    const i = h("input", { type: "checkbox", checked: on, style: "width:auto;margin-right:8px" });
    inputs[key] = i;
    return h("label", { style: "display:flex;align-items:center;color:var(--text)" }, i, label);
  };
  function render() {
    box.replaceChildren();
    inputs = {} as any;
    const k = sel.value as Kind;
    if (k === "local") {
      box.append(inp("path", "Folder on the server (leave empty for the data volume)", { value: initial?.backend === "local" ? initial.path ?? "" : "" }),
        h("p", { class: "muted" }, "Files stay on this server's disk. Use a cloud backend for production."));
    } else if (k === "gdrive") {
      box.append(
        inp("client_id", "OAuth client ID", { value: gd?.client_id }),
        inp("client_secret", "OAuth client secret", { type: "password", value: gd?.client_secret }),
        inp("refresh_token", "Refresh token (scope drive.file)", { type: "password", value: gd?.refresh_token }),
        inp("folder_name", "Folder name", { value: gd?.folder_name ?? "WorkChat" }),
        h("p", { class: "muted" }, "Files are stored in one app folder in your Drive. The token only reaches files this app created."));
    } else {
      const defaults: Record<string, { ep: string; region: string; style: boolean; hint: string }> = {
        s3: { ep: "", region: "us-east-1", style: false, hint: "Leave the endpoint empty for Amazon S3. For MinIO enter its URL and tick path-style." },
        r2: { ep: "https://<account-id>.r2.cloudflarestorage.com", region: "auto", style: false, hint: "Create an R2 API token with object read and write. Region is 'auto'." },
        b2: { ep: "https://s3.<region>.backblazeb2.com", region: "us-west-004", style: false, hint: "Use a Backblaze application key with access to the bucket (S3 compatible API)." },
      };
      const d = defaults[k];
      box.append(
        inp("endpoint", "Endpoint URL", { value: s3?.endpoint ?? d.ep, placeholder: d.ep }),
        inp("region", "Region", { value: s3?.region ?? d.region }),
        inp("bucket", "Bucket", { value: s3?.bucket }),
        inp("access_key_id", "Access key ID", { value: s3?.access_key_id }),
        inp("secret_access_key", "Secret access key", { type: "password", value: s3?.secret_access_key }),
        inp("prefix", "Folder prefix (optional)", { value: s3?.prefix ?? "" }),
        check("path_style", "Path-style requests (MinIO and some proxies)", s3?.path_style ?? false),
        check("allow_http", "Allow plain http (local testing only)", s3?.allow_http ?? false),
        h("p", { class: "muted" }, d.hint));
    }
  }
  sel.addEventListener("change", render);
  render();
  const val = (k: string) => (inputs[k] as HTMLInputElement | undefined)?.value.trim() ?? "";
  const chk = (k: string) => !!(inputs[k] as HTMLInputElement | undefined)?.checked;
  const el = h("div", {}, field("Storage backend", sel), box);
  return {
    el,
    value(): StorageConfig {
      const k = sel.value as Kind;
      if (k === "local") return { backend: "local", path: val("path") || null };
      if (k === "gdrive") return { backend: "gdrive", client_id: val("client_id"), client_secret: val("client_secret"), refresh_token: val("refresh_token"), folder_name: val("folder_name") || null };
      const provider = k === "s3" ? (val("endpoint") && val("endpoint").includes("127.0.0.1") ? "minio" : "aws") : k;
      return {
        backend: "s3", provider: provider as any, endpoint: val("endpoint") || null, region: val("region"), bucket: val("bucket"),
        access_key_id: val("access_key_id"), secret_access_key: val("secret_access_key"), prefix: val("prefix") || null,
        path_style: chk("path_style"), allow_http: chk("allow_http"),
      };
    },
  };
}
