import { describe, expect, it } from 'vitest'
import { defaultTarget, detectLang, guessLatinLang, LANG_NAMES } from '../lang-detect'

describe('语言检测（附录 8.1 规则）', () => {
  it('阿拉伯文字符 → 阿拉伯语', () => {
    expect(detectLang('مرحبا بالعالم')).toBe('ar')
  })

  it('谚文字符 → 韩语', () => {
    expect(detectLang('안녕하세요 세계')).toBe('ko')
  })

  it('西里尔字符 → 俄语', () => {
    expect(detectLang('Привет мир')).toBe('ru')
  })

  it('假名 → 日语', () => {
    expect(detectLang('こんにちは世界')).toBe('ja')
    expect(detectLang('カタカナ')).toBe('ja')
  })

  it('CJK 统一表意文字 → 中文', () => {
    expect(detectLang('你好世界，这是一段中文文本')).toBe('zh')
  })

  it('拉丁语系默认英语', () => {
    expect(detectLang('Hello world, this is a plain English sentence.')).toBe('en')
  })

  it('拉丁细分：西班牙语（ñ / ¿?）', () => {
    expect(guessLatinLang('El niño se preguntó ¿por qué?')).toBe('es')
  })

  it('拉丁细分：德语（ß / äöü + 高频词）', () => {
    expect(guessLatinLang('Der Fußball ist groß und schön')).toBe('de')
  })

  it('拉丁细分：法语（变音符 + 高频词）', () => {
    expect(guessLatinLang('Le garçon a mangé une très belle crème brûlée')).toBe('fr')
  })

  it('默认目标语言：英→中，中→英，其他→中', () => {
    expect(defaultTarget('en')).toBe('zh')
    expect(defaultTarget('zh')).toBe('en')
    expect(defaultTarget('ja')).toBe('zh')
    expect(defaultTarget('ar')).toBe('zh')
  })

  it('语言名称覆盖 FR-3.3 必需语言', () => {
    for (const code of ['zh', 'en', 'de', 'fr', 'ja', 'ar'] as const) {
      expect(LANG_NAMES[code]).toBeTruthy()
    }
  })
})
