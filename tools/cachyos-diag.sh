#!/usr/bin/env bash
# Диагностика Cursed Recode на Linux (CachyOS/Arch): версии wine и драйверов, vulkaninfo, запуск
# лоадера под wine с выводом в терминал, хвост loader.log. Итог — один файл, который можно показать
# тому, кто чинит.
#
#   tools/cachyos-diag.sh [путь/к/recode_loader.exe]
set -u

OUT="cursed-recode-diag-$(date +%Y%m%d-%H%M%S).txt"

{
echo "== система =="
uname -a
head -3 /etc/os-release 2>/dev/null

echo
echo "== пакеты (установленные) =="
for p in wine wine-staging winetricks vulkan-tools mesa-vulkan-drivers nvidia nvidia-dkms lib32-mesa-vulkan-drivers lib32-vulkan-radeon; do
    pacman -Q "$p" 2>/dev/null
done

echo
echo "== vulkan =="
vulkaninfo --summary 2>&1 | head -40 || echo "vulkaninfo нет (pacman -S vulkan-tools)"

echo
echo "== wine =="
wine --version 2>&1 || echo "wine нет (pacman -S wine)"
echo "WINEPREFIX=${WINEPREFIX:-$HOME/.wine (по умолчанию)}"

echo
echo "== место на диске =="
df -h "$HOME" | tail -1
} > "$OUT" 2>&1

LOADER="${1:-recode_loader.exe}"
if [ -f "$LOADER" ]; then
    {
    echo
    echo "== запуск $LOADER под wine (вывод первых 90 секунд) =="
    timeout 90 wine "$LOADER"
    echo "== код выхода: $? =="
    } >> "$OUT" 2>&1
    LOG="${WINEPREFIX:-$HOME/.wine}/dosdevices/c:/Cursed Recode/loader/loader.log"
    if [ -f "$LOG" ]; then
        {
        echo
        echo "== loader.log (последние 40 строк) =="
        tail -40 "$LOG"
        } >> "$OUT" 2>&1
    else
        echo "loader.log не найден: $LOG" >> "$OUT"
    fi
else
    echo "лоадер не найден: $LOADER (передайте путь: $0 путь/к/recode_loader.exe)" >> "$OUT"
fi

echo "готово: $OUT — откройте его и покажите тому, кто чинит"
