        // ===== دیکشنری نام فارسی کشورها =====
        const countryNamesEn = new Intl.DisplayNames(["en"], { type: "region" });
        const countryNamesFa = new Intl.DisplayNames(["fa"], { type: "region" });

        function getFaCountryName(code) {
            try {
                return countryNamesFa.of(code) || code;
            } catch (e) {
                return code;
            }
        }

        document.addEventListener('DOMContentLoaded', function() {
            const countInfo = document.getElementById('countInfo');
            const flagGrid = document.getElementById('flagGrid');
            const pageTitleText = document.getElementById('pageTitleText');
            const loadingText = document.getElementById('loadingText');
            const htmlRoot = document.getElementById('htmlRoot');
            const pulser = document.getElementById('pulser');

            function getFlagEmoji(countryCode) {
                const codePoints = countryCode
                    .toUpperCase()
                    .split('')
                    .map(char => 127397 + char.charCodeAt(0));
                return String.fromCodePoint(...codePoints);
            }

            function getCountryName(code) {
                try {
                    return countryNamesEn.of(code) || code;
                } catch (e) {
                    return code;
                }
            }

            // ===== ترجمه‌ها =====
            const i18n = {
                fa: {
                    title: 'کشورهایی که گره‌های خروجی تور دارند',
                    loading: 'در حال دریافت اطلاعات از سرور Tor...',
                    fetching: 'در حال دریافت اطلاعات از سرور Tor...',
                    noData: 'داده‌ای برای نمایش وجود ندارد',
                    error: 'خطا در دریافت داده',
                    countries: 'کشور',
                    relays: 'گره خروجی فعال',
                    copied: 'کپی شد!',
                    clickToCopy: 'برای کپی کلیک کنید',
                    copyCode: 'کد کشور کپی شد',
                    copyName: 'نام کشور کپی شد',
                    langTooltip: 'تغییر زبان',
                    themeTooltip: 'تغییر تم'
                },
                en: {
                    title: 'Countries with Tor Exit Nodes',
                    loading: 'Fetching data from Tor server...',
                    fetching: 'Fetching data from Tor server...',
                    noData: 'No data to display',
                    error: 'Error fetching data',
                    countries: 'countries',
                    relays: 'active exit relays',
                    copied: 'Copied!',
                    clickToCopy: 'Click to copy',
                    copyCode: 'Country code copied',
                    copyName: 'Country name copied',
                    langTooltip: 'Change language',
                    themeTooltip: 'Change theme'
                }
            };

            let currentLang = 'fa';
            let currentData = [];

            // ===== پارس کردن پارامترهای URL =====
            const urlParams = new URLSearchParams(window.location.search);
            const urlLang = urlParams.get('lang');
            if (urlLang === 'en' || urlLang === 'fa') {
                currentLang = urlLang;
            } else {
                const savedLang = localStorage.getItem('lang');
                if (savedLang === 'en' || savedLang === 'fa') {
                    currentLang = savedLang;
                }
            }

            let currentStatus = '';
            let currentErrorMessage = '';

            function t(key) {
                return i18n[currentLang][key] || key;
            }

            function updateStatusText() {
                if (currentStatus === 'fetching' && loadingText) {
                    loadingText.textContent = t('fetching');
                }
            }

            function updateDirection() {
                htmlRoot.setAttribute('dir', currentLang === 'fa' ? 'rtl' : 'ltr');
                htmlRoot.setAttribute('lang', currentLang === 'fa' ? 'fa' : 'en');
            }

            function updateTexts() {
                if (pageTitleText) pageTitleText.textContent = t('title');
                if (loadingText) loadingText.textContent = t('loading');
                document.getElementById('toggleLangBtn').title = t('langTooltip');
                document.getElementById('toggleThemeBtn').title = t('themeTooltip');
                const langLabelEl = document.getElementById('langLabel');
                if (langLabelEl) langLabelEl.textContent = currentLang === 'fa' ? 'EN' : 'FA';
                updateStatusText();
            }

            function renderCountries(data) {
                if (!data || data.length === 0) {
                    flagGrid.innerHTML = `
                        <div class="empty-state">
                            <span class="empty-icon">📭</span>
                            ${t('noData')}
                        </div>
                    `;
                    return;
                }
                let html = '';
                for (const country of data) {
                    const nameEn = country.name;
                    const nameFa = country.faName || country.name;
                    const displayName = currentLang === 'fa' ? nameFa : nameEn;

                    html += `
                            <div class="flag-item" data-code="${country.code}" data-name="${displayName}">
                                <span class="emoji">${country.flag}</span>
                                <span class="name" title="${t('clickToCopy')}" tabindex="0" role="button">${displayName}</span>
                                <span class="code" title="${t('clickToCopy')}" tabindex="0" role="button">${country.code}</span>
                                <span class="count">${country.count}</span>
                            </div>
                        `;
                }
                flagGrid.innerHTML = html;
                attachCopyHandlers();
            }

            // ===== کپی کردن با کلیک =====
            function copyToClipboard(text) {
                if (navigator.clipboard && navigator.clipboard.writeText) {
                    return navigator.clipboard.writeText(text);
                }
                return new Promise((resolve, reject) => {
                    const ta = document.createElement('textarea');
                    ta.value = text;
                    ta.style.position = 'fixed';
                    ta.style.left = '-9999px';
                    document.body.appendChild(ta);
                    ta.focus();
                    ta.select();
                    try {
                        const successful = document.execCommand('copy');
                        document.body.removeChild(ta);
                        if (successful) resolve();
                        else reject(new Error('copy failed'));
                    } catch (err) {
                        document.body.removeChild(ta);
                        reject(err);
                    }
                });
            }

            function showTooltipForElement(el, text) {
                const rect = el.getBoundingClientRect();
                const x = rect.left + rect.width / 2;
                const y = rect.top;
                showTooltip(x, y, text);
            }

            function attachCopyHandlers() {
                const items = document.querySelectorAll('.flag-item');
                items.forEach(item => {
                    // clicking the item (outside name/code) copies "Name (CC)"
                    item.addEventListener('click', function(e) {
                        const code = this.getAttribute('data-code');
                        const name = this.getAttribute('data-name');
                        const textToCopy = `${name} (${code})`;
                        copyToClipboard(textToCopy).then(() => {
                            const x = e.clientX || (this.getBoundingClientRect().left + this.getBoundingClientRect().width / 2);
                            const y = e.clientY || this.getBoundingClientRect().top;
                            showTooltip(x, y, t('copied'));
                            this.classList.add('copied');
                            setTimeout(() => this.classList.remove('copied'), 600);
                        }).catch(() => {
                            const x = e.clientX || (this.getBoundingClientRect().left + this.getBoundingClientRect().width / 2);
                            const y = e.clientY || this.getBoundingClientRect().top;
                            showTooltip(x, y, t('copied'));
                            this.classList.add('copied');
                            setTimeout(() => this.classList.remove('copied'), 600);
                        });
                    });

                    const codeEl = item.querySelector('.code');
                    if (codeEl) {
                        // hover/focus: show 'Click to copy'
                        codeEl.addEventListener('mouseenter', function(e) {
                            e.stopPropagation();
                            const rect = codeEl.getBoundingClientRect();
                            showTooltip(rect.left + rect.width / 2, rect.top, t('clickToCopy'), 0);
                        });
                        codeEl.addEventListener('mouseleave', function() { hideTooltip(); });
                        codeEl.addEventListener('focus', function(e) {
                            e.stopPropagation();
                            const rect = codeEl.getBoundingClientRect();
                            showTooltip(rect.left + rect.width / 2, rect.top, t('clickToCopy'), 0);
                        });
                        codeEl.addEventListener('blur', function() { hideTooltip(); });

                        codeEl.addEventListener('click', function(e) {
                            e.stopPropagation();
                            hideTooltip();
                            const code = item.getAttribute('data-code');
                            copyToClipboard(code).then(() => {
                                const x = e.clientX || (codeEl.getBoundingClientRect().left + codeEl.getBoundingClientRect().width / 2);
                                const y = e.clientY || codeEl.getBoundingClientRect().top;
                                showTooltip(x, y, t('copyCode'));
                                item.classList.add('copied');
                                setTimeout(() => item.classList.remove('copied'), 600);
                            }).catch(() => {
                                showTooltipForElement(codeEl, t('copyCode'));
                                item.classList.add('copied');
                                setTimeout(() => item.classList.remove('copied'), 600);
                            });
                        });
                        codeEl.addEventListener('keydown', function(e) {
                            if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
                                e.preventDefault();
                                this.click();
                            }
                        });
                    }

                    const nameEl = item.querySelector('.name');
                    if (nameEl) {
                        // hover/focus: show 'Click to copy'
                        nameEl.addEventListener('mouseenter', function(e) {
                            e.stopPropagation();
                            const rect = nameEl.getBoundingClientRect();
                            showTooltip(rect.left + rect.width / 2, rect.top, t('clickToCopy'), 0);
                        });
                        nameEl.addEventListener('mouseleave', function() { hideTooltip(); });
                        nameEl.addEventListener('focus', function(e) {
                            e.stopPropagation();
                            const rect = nameEl.getBoundingClientRect();
                            showTooltip(rect.left + rect.width / 2, rect.top, t('clickToCopy'), 0);
                        });
                        nameEl.addEventListener('blur', function() { hideTooltip(); });

                        nameEl.addEventListener('click', function(e) {
                            e.stopPropagation();
                            hideTooltip();
                            const name = item.getAttribute('data-name');
                            copyToClipboard(name).then(() => {
                                const x = e.clientX || (nameEl.getBoundingClientRect().left + nameEl.getBoundingClientRect().width / 2);
                                const y = e.clientY || nameEl.getBoundingClientRect().top;
                                showTooltip(x, y, t('copyName'));
                                item.classList.add('copied');
                                setTimeout(() => item.classList.remove('copied'), 600);
                            }).catch(() => {
                                showTooltipForElement(nameEl, t('copyName'));
                                item.classList.add('copied');
                                setTimeout(() => item.classList.remove('copied'), 600);
                            });
                        });
                        nameEl.addEventListener('keydown', function(e) {
                            if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
                                e.preventDefault();
                                this.click();
                            }
                        });
                    }
                });
            }

            let tooltipTimer = null;
            function showTooltip(x, y, text, duration = 1500) {
                const tooltip = document.getElementById('copyTooltip');
                if (tooltipTimer) {
                    clearTimeout(tooltipTimer);
                    tooltipTimer = null;
                }
                tooltip.textContent = text;
                tooltip.style.left = (x - tooltip.offsetWidth / 2) + 'px';
                tooltip.style.top = (y - 40) + 'px';
                tooltip.classList.add('show');
                if (duration > 0) {
                    tooltipTimer = setTimeout(() => {
                        tooltip.classList.remove('show');
                        tooltipTimer = null;
                    }, duration);
                }
            }

            function hideTooltip() {
                const tooltip = document.getElementById('copyTooltip');
                if (tooltipTimer) {
                    clearTimeout(tooltipTimer);
                    tooltipTimer = null;
                }
                tooltip.classList.remove('show');
            }

            // ===== دریافت داده =====
            async function fetchAndDisplayCountries() {
                try {
                    currentStatus = 'fetching';
                    currentErrorMessage = '';
                    updateStatusText();
                    countInfo.textContent = "";
                    if (pulser) pulser.classList.remove('hidden');
                    
                    let basePath = window.location.pathname;
                    if (basePath.endsWith('.html') || basePath.endsWith('.htm')) {
                        basePath = basePath.substring(0, basePath.lastIndexOf('/'));
                    }
                    if (!basePath.endsWith('/')) {
                        basePath += '/';
                    }
                    basePath = basePath.replace(/\/$/, '');
                    const API_BASE = basePath + '/api';

                    // Now fetches from cached backend proxy
                    const response = await fetch(`${API_BASE}/countries`);
                    if (!response.ok) {
                        throw new Error(`HTTP ${response.status}`);
                    }
                    const data = await response.json();
                    if (!data.relays) {
                        throw new Error("Invalid response from server");
                    }
                    const countries = {};
                    for (const relay of data.relays) {
                        if (!relay.country) continue;
                        const code = relay.country.toUpperCase();
                        if (!countries[code]) {
                            const nameEn = getCountryName(code);
                            countries[code] = {
                                code: code,
                                name: nameEn,
                                faName: getFaCountryName(code),
                                flag: getFlagEmoji(code),
                                count: 0
                            };
                        }
                        countries[code].count++;
                    }
                    const result = Object.values(countries)
                        .filter(item => item.count > 0)
                        .sort((a, b) => b.count - a.count);
                    currentData = result;
                    renderCountries(result);
                    const totalRelays = data.relays.length;
                    countInfo.textContent = `${result.length} ${t('countries')} | ${totalRelays} ${t('relays')}`;
                    currentStatus = 'complete';
                    updateStatusText();
                    // Pulser stays visible to indicate 'Live/Online' status
                } catch (err) {
                    console.error(err);
                    currentStatus = 'error';
                    currentErrorMessage = err.message;
                    updateStatusText();
                    countInfo.textContent = "";
                    if (pulser) pulser.classList.add('hidden');
                    flagGrid.innerHTML = `
                        <div class="empty-state">
                            <span class="empty-icon">⚠️</span>
                            ${t('error')}: ${err.message}
                        </div>
                    `;
                }
            }

            // ===== Event Listeners =====
            const toggleLangBtn = document.getElementById('toggleLangBtn');
            const langLabelEl = document.getElementById('langLabel');
            toggleLangBtn.addEventListener('click', function() {
                currentLang = currentLang === 'fa' ? 'en' : 'fa';
                updateDirection();
                updateTexts();
                renderCountries(currentData);
                if (langLabelEl) langLabelEl.textContent = currentLang === 'fa' ? 'EN' : 'FA';
                // Update count info text
                if (currentData.length > 0) {
                    const totalRelays = currentData.reduce((sum, c) => sum + c.count, 0);
                    countInfo.textContent = `${currentData.length} ${t('countries')} | ${totalRelays} ${t('relays')}`;
                }
            });

            // ===== تغییر تم =====
            const toggleThemeBtn = document.getElementById('toggleThemeBtn');
            const themeIcon = document.getElementById('themeIcon');
            let isDark = false;
            const urlTheme = urlParams.get('theme');
            let themeToUse = urlTheme;
            if (themeToUse !== 'dark' && themeToUse !== 'light') {
                themeToUse = localStorage.getItem('theme');
            }
            if (themeToUse === 'dark') {
                isDark = true;
                htmlRoot.setAttribute('data-theme', 'dark');
                themeIcon.textContent = '☀️';
            }
            toggleThemeBtn.addEventListener('click', function() {
                isDark = !isDark;
                if (isDark) {
                    htmlRoot.setAttribute('data-theme', 'dark');
                    themeIcon.textContent = '☀️';
                    localStorage.setItem('theme', 'dark');
                } else {
                    htmlRoot.removeAttribute('data-theme');
                    themeIcon.textContent = '🌙';
                    localStorage.setItem('theme', 'light');
                }
            });

            // ===== اجرای اصلی =====
            updateDirection();
            updateTexts();
            fetchAndDisplayCountries();
        });
