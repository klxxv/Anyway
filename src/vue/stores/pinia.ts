import { createPinia, type Pinia } from "pinia";

/** The single Pinia root instance shared by the Vue application. */
export const pinia: Pinia = createPinia();

/**
 * Return the application Pinia instance for stores used outside components.
 * Keeping this helper centralized avoids creating multiple roots.
 */
export function getPinia(): Pinia {
  return pinia;
}
