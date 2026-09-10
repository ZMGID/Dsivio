import { Fragment } from 'react'
import {
  Blocks,
  BookOpen,
  ChevronRight,
  FolderGit2,
  MessageSquare,
  ScanSearch,
  Terminal,
  Upload,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { I18n } from '../../settings/i18n'
import { OnboardingStepFrame } from '../OnboardingStepFrame'
import { Button } from '../../components/Button'

type WelcomeStepProps = {
  t: I18n
  onImportConfig: () => void
  importing: boolean
  disabled: boolean
  importError: string | null
}

type FeatureCard = {
  icon: LucideIcon
  title: string
  desc: string
}

export function WelcomeStep({ t, onImportConfig, importing, disabled, importError }: WelcomeStepProps) {
  const features: FeatureCard[] = [
    { icon: MessageSquare, title: t.onboardingWelcomeChatTitle, desc: t.onboardingWelcomeChatDesc },
    { icon: Terminal, title: t.onboardingWelcomeCliAgentTitle, desc: t.onboardingWelcomeCliAgentDesc },
    { icon: Blocks, title: t.onboardingWelcomeExtensionsTitle, desc: t.onboardingWelcomeExtensionsDesc },
    { icon: BookOpen, title: t.onboardingWelcomeKnowledgeTitle, desc: t.onboardingWelcomeKnowledgeDesc },
    { icon: ScanSearch, title: t.onboardingWelcomeLensTitle, desc: t.onboardingWelcomeLensDesc },
    { icon: FolderGit2, title: t.onboardingWelcomeWorkspaceTitle, desc: t.onboardingWelcomeWorkspaceDesc },
  ]

  const setupSteps = [
    t.onboardingWelcomeStepProvider,
    t.onboardingWelcomeStepWebSearch,
    t.onboardingWelcomeStepHotkey,
  ]

  return (
    <OnboardingStepFrame title={t.onboardingWelcomeTitle} subtitle={t.onboardingWelcomeSubtitle}>
      <section className="onboarding-card flex flex-col gap-3" aria-busy={importing}>
        <div className="onboarding-field-copy">
          <h2 className="onboarding-field-label">{t.onboardingImportTitle}</h2>
          <p className="onboarding-field-hint">{t.onboardingImportDesc}</p>
        </div>
        <div>
          <Button variant="primary" onClick={onImportConfig} disabled={disabled}>
            <Upload size={15} />
            {importing ? t.onboardingImportBusy : t.onboardingImportButton}
          </Button>
        </div>
        {importError ? (
          <p className="onboarding-panel-note text-red-600 dark:text-red-400" role="alert">
            {t.onboardingImportFailed} {importError}
          </p>
        ) : null}
      </section>
      <div className="onboarding-section">
        <div className="onboarding-section-label">{t.onboardingWelcomeSectionFeatures}</div>
        <div className="onboarding-feature-grid">
          {features.map(({ icon: Icon, title, desc }) => (
            <article key={title} className="onboarding-feature-card">
              <div className="onboarding-feature-icon">
                <Icon size={18} strokeWidth={1.75} />
              </div>
              <div className="onboarding-feature-copy">
                <h3 className="onboarding-field-label">{title}</h3>
                <p className="onboarding-field-hint">{desc}</p>
              </div>
            </article>
          ))}
        </div>
      </div>

      <div className="onboarding-section">
        <div className="onboarding-section-label">{t.onboardingWelcomeSectionSetup}</div>
        <div className="onboarding-setup-roadmap">
          {setupSteps.map((label, index) => (
            <Fragment key={label}>
              {index > 0 ? <ChevronRight size={14} className="onboarding-setup-arrow" /> : null}
              <div className="onboarding-setup-step">
                <span className="onboarding-setup-step-index">{index + 1}</span>
                <span className="onboarding-setup-step-label">{label}</span>
              </div>
            </Fragment>
          ))}
        </div>
        <p className="onboarding-panel-note">{t.onboardingWelcomeFootnote}</p>
      </div>
    </OnboardingStepFrame>
  )
}
