import { expect, test } from '@playwright/test';

// Runs the built app against the in-memory mock backend (no Tauri).
test('first run, read worlds from a phone, export them', async ({ page }) => {
  await page.goto('/?scenario=fresh&tick=20');
  await page.getByText('I understand').click();
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.getByRole('heading', { name: 'Devices', level: 1 })).toBeVisible();
  await page.getByRole('button', { name: 'Read worlds' }).first().click();
  await page.getByText('The game is closed').click();
  await page.getByRole('button', { name: 'Start reading' }).click();
  await expect(page.getByText('Look at your phone and tap “Back up my data”')).toBeVisible();
  await expect(page.getByRole('table', { name: 'Worlds on Ada’s Pixel 8' })).toBeVisible();
  await page.getByRole('button', { name: /Export 5 worlds/ }).click();
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  await expect(page.getByRole('heading', { name: '5 worlds exported' })).toBeVisible();
  await page.getByRole('button', { name: 'Open in Worlds' }).click();
  await expect(page.getByRole('listitem', { name: 'Sunny Isle' })).toBeVisible();
});

test('small window uses the icon rail; the language switches to Russian', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 640 });
  await page.goto('/');
  await expect(page.getByRole('navigation', { name: 'Main' })).toHaveCSS('width', '76px');
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByRole('combobox', { name: 'Language' }).selectOption('Русский');
  await expect(page.getByRole('heading', { name: 'Настройки', level: 1 })).toBeVisible();
});
