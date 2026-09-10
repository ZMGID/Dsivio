import catalog from './company-skills.json'

/** Curated by the company; packages live in resources/company-skills, outside auto-scanned skills. */
export type CompanySkill = {
  id: string
  name: string
  description: string
  category: string
  version: string
  author: string
  details: string
  /** Directory relative to the bundled company-skills resource directory. */
  directory: string
}

export const companySkills: readonly CompanySkill[] = catalog
