// Curated «рекомендуемые модели» — не HF trending, а ручной список (обновлять раз в месяц).
// GGUF: unsloth, конкретные файлы проверены на HF (2026-07).
// Смысл: новичок выбирает «зачем», а не «какой Qwen».

export type RecCategory = "light" | "chat" | "smart" | "code" | "power";

export interface RecommendedModel {
  id: string;
  /** i18n ключ заголовка */
  titleKey: string;
  /** i18n ключ короткого описания */
  blurbKey: string;
  category: RecCategory;
  /** Человекочитаемая линейка: «Qwen 3.6», «Gemma 4» */
  family: string;
  /** Ярлык размера: «27B», «12B» */
  sizeLabel: string;
  hfRepo: string;
  /** Конкретный .gguf в корне репо */
  file: string;
  /** Размер файла (байт), для UI и проверки места */
  fileBytes: number;
  /** Оценка VRAM для комфортного запуска Q4 (байт) */
  vramHintBytes: number;
  /** Показать бейдж «Лучший выбор» */
  featured?: boolean;
}

const GB = 1024 ** 3;

/**
 * Актуальный срез середины 2026:
 * Qwen 3.6 + Gemma 4 + лёгкие Phi/Qwen3.5.
 * Не тащим Llama 3 / Qwen 2.5 / Gemma 3 как «рекомендуем».
 */
export const RECOMMENDED_MODELS: RecommendedModel[] = [
  {
    id: "phi4-mini",
    titleKey: "rec.phi4_mini.title",
    blurbKey: "rec.phi4_mini.blurb",
    category: "light",
    family: "Phi-4",
    sizeLabel: "3.8B",
    hfRepo: "unsloth/Phi-4-mini-instruct-GGUF",
    file: "Phi-4-mini-instruct-Q4_K_M.gguf",
    fileBytes: Math.round(2.376 * GB),
    vramHintBytes: Math.round(4 * GB),
  },
  {
    id: "qwen35-4b",
    titleKey: "rec.qwen35_4b.title",
    blurbKey: "rec.qwen35_4b.blurb",
    category: "light",
    family: "Qwen 3.5",
    sizeLabel: "4B",
    hfRepo: "unsloth/Qwen3.5-4B-GGUF",
    file: "Qwen3.5-4B-Q4_K_M.gguf",
    fileBytes: Math.round(2.614 * GB),
    vramHintBytes: Math.round(5 * GB),
  },
  {
    id: "gemma4-e4b",
    titleKey: "rec.gemma4_e4b.title",
    blurbKey: "rec.gemma4_e4b.blurb",
    category: "chat",
    family: "Gemma 4",
    sizeLabel: "E4B",
    hfRepo: "unsloth/gemma-4-E4B-it-GGUF",
    file: "gemma-4-E4B-it-Q4_K_M.gguf",
    fileBytes: Math.round(4.747 * GB),
    vramHintBytes: Math.round(7 * GB),
  },
  {
    id: "gemma4-12b",
    titleKey: "rec.gemma4_12b.title",
    blurbKey: "rec.gemma4_12b.blurb",
    category: "chat",
    family: "Gemma 4",
    sizeLabel: "12B",
    hfRepo: "unsloth/gemma-4-12b-it-GGUF",
    file: "gemma-4-12b-it-Q4_K_M.gguf",
    fileBytes: Math.round(6.792 * GB),
    vramHintBytes: Math.round(10 * GB),
  },
  {
    id: "qwen36-27b",
    titleKey: "rec.qwen36_27b.title",
    blurbKey: "rec.qwen36_27b.blurb",
    category: "smart",
    family: "Qwen 3.6",
    sizeLabel: "27B",
    hfRepo: "unsloth/Qwen3.6-27B-GGUF",
    file: "Qwen3.6-27B-Q4_K_M.gguf",
    fileBytes: Math.round(16.038 * GB),
    vramHintBytes: Math.round(18 * GB),
    featured: true,
  },
  {
    id: "qwen36-27b-code",
    titleKey: "rec.qwen36_code.title",
    blurbKey: "rec.qwen36_code.blurb",
    category: "code",
    family: "Qwen 3.6",
    sizeLabel: "27B",
    hfRepo: "unsloth/Qwen3.6-27B-GGUF",
    file: "Qwen3.6-27B-Q4_K_M.gguf",
    fileBytes: Math.round(16.038 * GB),
    vramHintBytes: Math.round(18 * GB),
  },
  {
    id: "qwen36-35b-a3b",
    titleKey: "rec.qwen36_35b.title",
    blurbKey: "rec.qwen36_35b.blurb",
    category: "power",
    family: "Qwen 3.6",
    sizeLabel: "35B-A3B",
    hfRepo: "unsloth/Qwen3.6-35B-A3B-GGUF",
    file: "Qwen3.6-35B-A3B-UD-Q4_K_M.gguf",
    fileBytes: Math.round(21.109 * GB),
    vramHintBytes: Math.round(20 * GB),
  },
];

export const REC_CATEGORIES: { id: RecCategory | "all"; labelKey: string }[] = [
  { id: "all", labelKey: "rec.cat.all" },
  { id: "light", labelKey: "rec.cat.light" },
  { id: "chat", labelKey: "rec.cat.chat" },
  { id: "smart", labelKey: "rec.cat.smart" },
  { id: "code", labelKey: "rec.cat.code" },
  { id: "power", labelKey: "rec.cat.power" },
];

export type FitLevel = "ok" | "tight" | "no" | "unknown";

/** Сравнение с VRAM GPU (или RAM, если GPU нет). */
export function fitLevel(
  model: RecommendedModel,
  vramBytes: number | null,
  ramBytes: number | null,
): FitLevel {
  const budget =
    vramBytes && vramBytes > 0
      ? vramBytes
      : ramBytes && ramBytes > 0
        ? Math.max(0, ramBytes - 4 * GB) // ОС + UI
        : null;
  if (budget == null) return "unknown";
  if (budget >= model.vramHintBytes) return "ok";
  if (budget >= model.vramHintBytes * 0.7) return "tight";
  return "no";
}
