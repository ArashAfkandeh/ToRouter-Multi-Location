import Alpine from 'alpinejs';
import { apiCall } from './api.js';
import { auth } from './auth.js';
import { t } from './i18n.js';
import { nodesService } from './nodes.js';
import { settingsService } from './settings.js';
import { logsService } from './logs.js';
import { polyfillCountryFlagEmojis } from "country-flag-emoji-polyfill";

polyfillCountryFlagEmojis();

window.Alpine = Alpine;
window.alpineApiCall = apiCall;
window.nodesService = nodesService;
window.settingsService = settingsService;
window.logsService = logsService;

window.formatLog = function(line) {
    let html = line.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    html = html.replace(/\x1b\[32m/g, '<span class="text-emerald-400">')
               .replace(/\x1b\[34m/g, '<span class="text-blue-400">')
               .replace(/\x1b\[33m/g, '<span class="text-yellow-400">')
               .replace(/\x1b\[31m/g, '<span class="text-red-400">')
               .replace(/\x1b\[36m/g, '<span class="text-cyan-400">')
               .replace(/\x1b\[1m/g, '<span class="font-bold">')
               .replace(/\x1b\[2m/g, '<span class="text-slate-500">')
               .replace(/\x1b\[0m/g, '</span>')
               .replace(/\x1b\[[0-9;]*m/g, '');
    return html;
};
window.backupDatabase = function() {
    let basePath = window.location.pathname;
    if (basePath.endsWith('.html') || basePath.endsWith('.htm')) basePath = basePath.substring(0, basePath.lastIndexOf('/'));
    if (basePath.endsWith('/')) basePath = basePath.substring(0, basePath.length - 1);
    window.location.href = `${basePath}/api/backup`;
};

window.handleRestoreFile = async function(e) {
    const file = e.target.files[0];
    if (!file) return;
    if (!confirm('Are you sure you want to restore the database? This will overwrite your current settings and routes, and restart the ToRouter service.')) {
        e.target.value = '';
        return;
    }
    
    Alpine.store('toast').show('Uploading database...', 'info');
    
    try {
        const formData = new FormData();
        formData.append('file', file);
        
        let basePath = window.location.pathname;
        if (basePath.endsWith('.html') || basePath.endsWith('.htm')) basePath = basePath.substring(0, basePath.lastIndexOf('/'));
        if (basePath.endsWith('/')) basePath = basePath.substring(0, basePath.length - 1);
        
        const res = await fetch(`${basePath}/api/restore`, {
            method: 'POST',
            body: formData
        });
        
        if (res.ok) {
            Alpine.store('toast').show('Restore successful! Restarting daemon, please wait...', 'success');
            setTimeout(() => {
                window.location.reload();
            }, 5000);
        } else {
            const text = await res.text();
            Alpine.store('toast').show('Restore failed: ' + text, 'error');
        }
    } catch (error) {
        Alpine.store('toast').show('Network error during restore', 'error');
    }
    
    e.target.value = '';
};

Alpine.store('app', {
    lang: localStorage.getItem('lang') || 'en',
    theme: localStorage.getItem('theme') || 'dark',
    updateInterval: parseInt(localStorage.getItem('updateInterval')) || 15,
    isAuthenticated: false,
    isInitializing: true,
    version: '...',
    nodes: [],
    metrics: { total: 0, healthy: 0, error: 0 },
    ws: null,

    init() {
        console.log('[DEBUG] App init started, isInitializing:', this.isInitializing);
        this.applyTheme();
        this.checkAuth();
        this.fetchGithubStars();
        this.fetchVersion();
    },

    async fetchVersion() {
        try {
            const basePath = window.location.pathname.replace(/\/$/, '');
            const response = await fetch(`${basePath}/api/version`);
            if (response.ok) {
                this.version = await response.text();
            }
        } catch (e) {
            console.error('Failed to fetch version', e);
        }
    },

    async fetchGithubStars() {
        try {
            const response = await fetch('https://api.github.com/repos/ArashAfkandeh/ToRouter-Multi-Location');
            if (response.ok) {
                const data = await response.json();
                const el = document.getElementById('github-stars-count');
                if (el) el.textContent = '(' + data.stargazers_count + ')';
            }
        } catch (e) {
            console.error('Failed to fetch github stars', e);
        }
    },

    connectWs() {
        if (this.ws) return;
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let basePath = window.location.pathname;
        if (basePath.endsWith('.html') || basePath.endsWith('.htm')) basePath = basePath.substring(0, basePath.lastIndexOf('/'));
        if (basePath.endsWith('/')) basePath = basePath.substring(0, basePath.length - 1);

        const wsUrl = `${protocol}//${window.location.host}${basePath}/api/ws`;

        this.ws = new WebSocket(wsUrl);

        this.ws.onmessage = (event) => {
            try {
                const payload = JSON.parse(event.data);
                if (payload.routes) {
                    const localNodesMap = {};
                    if (this.nodes) {
                        this.nodes.forEach(n => { localNodesMap[n.id] = n; });
                    }

                    this.nodes = payload.routes.map(serverNode => {
                        const localNode = localNodesMap[serverNode.id];
                        if (localNode && localNode._frontendProbed) {
                            serverNode.latency = localNode.latency;
                            serverNode.status = localNode.status;
                            if (localNode.tor_ip) serverNode.tor_ip = localNode.tor_ip;
                            serverNode._frontendProbed = true;
                        }
                        return serverNode;
                    });

                    this.metrics.total = this.nodes.length;
                    this.metrics.healthy = this.nodes.filter(n => n.status === 'healthy').length;
                    this.metrics.error = this.nodes.filter(n => n.status === 'error').length;
                }
                if (payload.logs) {
                    const logsArray = Array.isArray(payload.logs) ? payload.logs : payload.logs.split('\n');
                    const filtered = logsArray.filter(l => l && l.trim().length > 0);
                    // Emit a custom event so the logs modal can update itself without polling
                    window.dispatchEvent(new CustomEvent('logs-updated', { detail: filtered }));
                }
            } catch (e) {
                console.error('Failed to parse WS message', e);
            }
        };

        this.ws.onclose = () => {
            this.ws = null;
            if (this.isAuthenticated) {
                setTimeout(() => this.connectWs(), 3000);
            }
        };
    },

    t(key) {
        return t(key, this.lang);
    },

    toggleLang() {
        this.lang = this.lang === 'en' ? 'fa' : 'en';
        localStorage.setItem('lang', this.lang);
        document.body.setAttribute('dir', this.lang === 'fa' ? 'rtl' : 'ltr');
    },

    toggleTheme() {
        this.theme = this.theme === 'dark' ? 'light' : 'dark';
        localStorage.setItem('theme', this.theme);
        this.applyTheme();
    },

    applyTheme() {
        if (this.theme === 'dark') {
            document.documentElement.classList.add('dark');
        } else {
            document.documentElement.classList.remove('dark');
        }
    },

    async checkAuth() {
        console.log('[DEBUG] checkAuth started');
        const isAuth = await auth.check();
        console.log('[DEBUG] Auth check result:', isAuth);
        this.isAuthenticated = isAuth;
        this.isInitializing = false;
        console.log('[DEBUG] isInitializing set to false');
        if (isAuth) {
            this.fetchNodes();
            this.connectWs();
            this.startPeriodicFetch();
        }
    },

    async login(username, password, errorCallback) {
        const res = await auth.login(username, password);
        if (res.error) {
            errorCallback(res.error);
        } else {
            this.isAuthenticated = true;
            this.fetchNodes();
            this.connectWs();
            this.startPeriodicFetch();
        }
    },

    logout() {
        auth.logout();
        this.isAuthenticated = false;
        this.nodes = [];
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        if (this.fetchInterval) {
            clearInterval(this.fetchInterval);
            this.fetchInterval = null;
        }
    },

    setUpdateInterval(seconds) {
        this.updateInterval = seconds;
        localStorage.setItem('updateInterval', seconds);
        this.startPeriodicFetch();
    },

    startPeriodicFetch() {
        if (this.fetchInterval) clearInterval(this.fetchInterval);
        this.fetchInterval = setInterval(() => {
            if (this.isAuthenticated) {
                this.fetchNodes();
            }
        }, this.updateInterval * 1000);
    },

    async fetchNodes() {
        const res = await nodesService.fetchNodes();
        if (!res.error && res.data) {
            const data = res.data;
            this.nodes = data;
            this.metrics.total = data.length;
            this.metrics.healthy = data.filter(n => n.status === 'healthy').length;
            this.metrics.error = data.filter(n => n.status === 'error').length;
        }
    },

    getCountryFlagEmoji(countryCode) {
        if (!countryCode) return '';
        const match = String(countryCode).trim().toUpperCase().match(/[A-Z]{2}/);
        if (!match) return '';
        return match[0].split('').map(char => String.fromCodePoint(0x1F1E6 + char.charCodeAt(0) - 65)).join('');
    },

    formatTime(ts) {
        if (!ts) return '-';
        return new Date(Number(ts)).toLocaleTimeString([], { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
    },

    async restartNode(id) {
        Alpine.store('toast').show(`Restarting...`, 'info');
        const res = await nodesService.restartNode(id);
        if (res.error) Alpine.store('toast').show(`Failed to restart: ${res.error}`, 'error');
        else {
            Alpine.store('toast').show(`Node restarted.`, 'success');
            this.fetchNodes();
        }
    },

    async deleteNode(id) {
        const res = await nodesService.deleteNode(id);
        if (res.error) {
            Alpine.store('toast').show(`Failed to delete: ${res.error}`, 'error');
            return false;
        } else {
            Alpine.store('toast').show(`Node deleted.`, 'success');
            this.fetchNodes();
            return true;
        }
    }
});

Alpine.store('toast', {
    toasts: [],
    show(message, type = 'info') {
        const id = Date.now();
        this.toasts.push({ id, message, type });
        setTimeout(() => {
            this.toasts = this.toasts.filter(t => t.id !== id);
        }, 3000);
    }
});

Alpine.start();
