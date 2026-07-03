import { defineConfig } from 'vite';
import { fileURLToPath } from 'url';
import path from 'path';
import fs from 'fs';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

function htmlIncludePlugin() {
  return {
    name: 'html-include',
    enforce: 'pre',
    transformIndexHtml(html) {
      return html.replace(/<include\s+src="([^"]+)"\s*><\/include>/g, (match, src) => {
        const filePath = path.resolve(__dirname, src);
        if (fs.existsSync(filePath)) {
          return fs.readFileSync(filePath, 'utf-8');
        }
        return match;
      });
    }
  }
}

export default defineConfig({
  plugins: [htmlIncludePlugin()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, 'index.html'),
        countries: path.resolve(__dirname, 'Countries.html')
      }
    }
  },
  base: './', // Use relative paths
});
