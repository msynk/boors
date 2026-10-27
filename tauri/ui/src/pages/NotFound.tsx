import { A } from '@solidjs/router';
import { t } from '../i18n';

export default function NotFound() {
  return (
    <div class="page">
      <div class="state empty not-found">
        <div>
          <h1 class="page-title">{t('notFoundTitle')}</h1>
          <p>{t('notFoundBody')}</p>
          <A href="/market" class="btn primary">
            {t('goToOverview')}
          </A>
        </div>
      </div>
    </div>
  );
}
