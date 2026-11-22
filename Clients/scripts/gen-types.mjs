// Generates src/lib/api-types.ts from schema/api.schema.json (the vendored copy of the server's schema).
import { compileFromFile } from "json-schema-to-typescript";
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const ts = await compileFromFile(join(root, "schema/api.schema.json"), {
  additionalProperties: false,
  bannerComment: "/* Generated from schema/api.schema.json by `npm run gen-types`. Do not edit. */",
  style: { singleQuote: false },
});
writeFileSync(join(root, "src/lib/api-types.ts"), ts);
console.log("wrote src/lib/api-types.ts");
