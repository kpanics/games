"use strict";
(async () => {
  const id = new URLSearchParams(location.search).get('id');
  if (!id || !/^[A-Za-z0-9-]{1,80}$/.test(id)) return;
  const heading = document.querySelector('h1'), message = document.querySelector('p');
  for (let attempt = 0; attempt < 12; attempt++) {
    try {
      const response = await fetch('/api/pay/pally/status?id=' + encodeURIComponent(id), { credentials: 'same-origin', cache: 'no-store' });
      if (response.status === 401) { message.textContent = 'Войди в личный кабинет, чтобы проверить подписку. После подтверждения оплаты она начислится автоматически.'; return; }
      if (response.status === 404) { message.textContent = 'Заказ не найден для этого аккаунта. Проверь, что вошёл в аккаунт, с которого оплачивал.'; return; }
      if (response.ok && (await response.json()).paid) {
        heading.textContent = 'Оплата подтверждена';
        message.textContent = 'Подписка Cursed Recode начислена на твой аккаунт. Можно перейти в личный кабинет.';
        return;
      }
    } catch (_) { /* Keep the honest pending text, and retry briefly. */ }
    await new Promise(resolve => setTimeout(resolve, 11000));
  }
  message.textContent = 'Подтверждение пока не получено. Если деньги списались, не оплачивай повторно: проверь личный кабинет чуть позже или обратись в поддержку.';
})();
