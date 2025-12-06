import { exit } from "process";
import * as fsp from "fs/promises";
import path from "path";
import * as fs from "fs";
import ollama from 'ollama'

const model_name = process.env["MODEL"] as string;
const template_name = process.env["TEMPLATE"] as string;
let lang_path = process.env["LANG_PATH"] as string;

if (model_name === undefined) {
  console.log("MODEL not set");
  exit(-1);
}

if (template_name === undefined) {
  console.log("TEMPLATE not set");
  exit(-1);
}

if (lang_path === undefined) {
  console.log("LANG_PATH is not set. Try one of these:\n\nLANG_PATH=../../uidev/assets/lang/ ./run.sh\nLANG_PATH=../../dash-frontend/assets/lang/ ./run.sh\n");
  exit(-1);
}

lang_path = path.resolve(__dirname + "/" + lang_path);
if (lang_path === undefined || !fs.existsSync(lang_path)) {
  console.log("Invalid or non-existent LANG_PATH");
  exit(-1);
}

const current_path = path.resolve(__dirname);
const templates_path = path.resolve(__dirname + "/templates");

async function loop_object(obj: any, initial_str: string, callback: (key: string, value: string) => Promise<void>) {
  for (var key in obj) {
    let full_key = initial_str + key;
    if (typeof obj[key] === "object" && obj[key] !== null) {
      await loop_object(obj[key], full_key + ".", callback)
    } else if (obj.hasOwnProperty(key)) {
      await callback(full_key, obj[key])
    }
  }
}

function extract_backticks(str: string) {
  const regex = /`([^`]+)`/g;
  return str.match(regex)?.map(match => match.slice(1, -1).trim());
}

function set_i18n_key(obj: any, key: string, value: string | undefined) {
  const parts = key.split('.');
  let cur_level = obj;
  for (let i = 0; i < parts.length - 1; i++) {
    const part = parts[i]!;
    if (!cur_level[part]) {
      cur_level[part] = {};
    }
    cur_level = cur_level[part];
  }
  cur_level[parts[parts.length - 1]!] = value;
}

function key_exists(obj: any, key: string) {
  const parts = key.split('.');
  let level = obj;

  for (let i = 0; i < parts.length; i++) {
    const part = parts[i]!;
    if (!level || !level[part]) {
      return false;
    }
    level = level[part];
  }

  return true;
};

interface Example {
  key: string;
  en: string;
  translated: string;
}

interface Template {
  full_name: string; // "Polish"
  examples: Example[]
}

function gen_prompt(description: string, template: Template, key: string, english_translation: string) {
  let num = 1;
  for (const example of template.examples) {
    description += "\nExample " + num + ":\n\n";
    description += "Translate key `" + example.key + "` from English to " + template.full_name + ":\n\n";
    description += "```\n";
    description += example.en + "\n";
    description += "```\n\n";
    description += "Result:\n\n";
    description += "```\n";
    description += example.translated + "\n";
    description += "```\n";
    num += 1;
  }
  description += "\nEnd of examples.\n\n";
  description += "Translate key `" + key + "` from English to " + template.full_name + ":\n\n";
  description += "```\n";
  description += english_translation + "\n";
  description += "```\n";
  return description;
}

async function run() {
  const template = JSON.parse(await fsp.readFile(templates_path + "/" + template_name + ".json", "utf-8")) as Template;

  let description_txt = await fsp.readFile(current_path + "/description.txt", "utf-8");
  description_txt = description_txt.replaceAll("{TARGET_LANG}", template.full_name);

  const orig_english_json = JSON.parse(await fsp.readFile(lang_path + "/en.json") as any);

  let orig_translated_json = {};
  try {
    orig_translated_json = JSON.parse((await fsp.readFile(lang_path + "/" + template_name + ".json")).toString());
  }
  catch (_e) { }

  let llm_translated_json = {};
  const translated_json_path = lang_path + "/" + template_name + ".json";
  if (await fsp.exists(translated_json_path)) {
    llm_translated_json = JSON.parse((await fsp.readFile(translated_json_path)).toString());
  }

  let human = 0;
  let llm = 0;

  let total_count = 0;
  await loop_object(orig_english_json, "", async () => {
    total_count += 1;
  });

  await loop_object(llm_translated_json, "", async (key, _) => {
    if (!key_exists(orig_english_json, key)) {
      console.log("Removing key", key);
      set_i18n_key(llm_translated_json, key, undefined);
      fsp.writeFile(translated_json_path, JSON.stringify(llm_translated_json, undefined, 2));
    }
  });

  await loop_object(orig_english_json, "", async (key, english_translation) => {
    if (key_exists(orig_translated_json, key)) {
      human += 1;
      return;
    }

    if (key_exists(llm_translated_json, key)) {
      llm += 1;
      return;
    }

    console.log("Translating", key, "...");
    llm++;

    const prompt = gen_prompt(description_txt, template, key, english_translation);

    const response = await ollama.chat({
      model: model_name,
      messages: [{ role: "user", content: prompt }],
      options: {
        seed: 12345,
      }
    })

    const msg = extract_backticks(response.message.content);
    if (msg === undefined || msg[0] === undefined) {
      throw new Error("backticks failed. Raw content: " + response.message.content);
    }

    console.log(" >>>", msg);

    set_i18n_key(llm_translated_json, key, msg[0]);
    fsp.writeFile(translated_json_path, JSON.stringify(llm_translated_json, undefined, 2));
  });

  console.log("\"" + template_name + "\" translation finished,", human, "were already human translated,", llm, "llm-translated (" + Math.round((llm / total_count) * 100.0) + "% machine-translated)");
}

run().catch((e) => {
  console.log("Fatal error:", e);
  exit(-1);
});