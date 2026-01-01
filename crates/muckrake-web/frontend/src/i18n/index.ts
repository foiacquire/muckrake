import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import LanguageDetector from 'i18next-browser-languagedetector';

import en from './locales/en.json';

const resources = {
  en: { translation: en },
};

export const RTL_LANGUAGES = ['ar', 'he', 'fa', 'ur'];

export function getDirection(lang: string): 'ltr' | 'rtl' {
  return RTL_LANGUAGES.includes(lang.split('-')[0]) ? 'rtl' : 'ltr';
}

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: 'en',
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ['localStorage', 'navigator'],
      caches: ['localStorage'],
    },
  });

i18n.on('languageChanged', (lng) => {
  document.documentElement.dir = getDirection(lng);
  document.documentElement.lang = lng;
});

document.documentElement.dir = getDirection(i18n.language);
document.documentElement.lang = i18n.language;

export default i18n;
