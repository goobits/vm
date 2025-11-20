# 🎨 Custom Themes Guide

Create custom color schemes for `@goobits/themes` to match your brand and design requirements.

## 📦 Included Presets

This package includes 2 ready-to-use color schemes:

- **default** - Clean, professional design system with subtle effects
- **spells** - Magical purple theme with enhanced animations and glow effects

## 🚀 Quick Start

### Using Preset Themes

```typescript
// Import preset themes in your root layout
import '@goobits/themes/themes/default.css';
import '@goobits/themes/themes/spells.css';
```

### Creating a Custom Theme

1. Create a new CSS file in your project: `src/styles/themes/ocean.css`
2. Define your scheme with CSS custom properties
3. Import it in your layout
4. Add the scheme to your theme config

## 📐 CSS Structure

Themes use CSS custom properties (CSS variables) that override the base design tokens. Each theme is scoped to a class name matching the pattern: `.scheme-{name}`

### Basic Theme Structure

```css
/* src/styles/themes/ocean.css */

html.scheme-ocean {
    /* Primary color palette */
    --accent-primary: #0066cc;
    --accent-glow: #00ccff;
    --accent-secondary: #0088ee;

    /* Override brand colors */
    --brand-gradient-start: #0066cc;
    --brand-gradient-end: #00ccff;
    --brand-gradient: linear-gradient(
        135deg,
        var(--brand-gradient-start),
        var(--brand-gradient-end)
    );

    /* Primary color scale */
    --color-primary-500: #0066cc;
    --color-primary-600: #0055aa;
    --color-primary-700: #004488;
}

/* Dark mode overrides */
html.theme-dark.scheme-ocean,
html.theme-system-dark.scheme-ocean {
    --bg-primary: #001a33;
    --bg-secondary: #002244;
    --text-primary: #e0f2ff;
}
```

## 🎨 CSS Variables Reference

### Core Color Variables

| Variable             | Purpose              | Example                   |
| -------------------- | -------------------- | ------------------------- |
| `--accent-primary`   | Primary accent color | `#7c3aed`                 |
| `--accent-glow`      | Glow/highlight color | `#a78bfa`                 |
| `--accent-secondary` | Secondary accent     | `#8b5cf6`                 |
| `--bg-primary`       | Main background      | `#0a0a0f`                 |
| `--bg-secondary`     | Secondary background | `#13131f`                 |
| `--bg-tertiary`      | Tertiary background  | `#1a1a2e`                 |
| `--text-primary`     | Primary text color   | `#e0e0ff`                 |
| `--text-secondary`   | Secondary text       | `#9090b0`                 |
| `--border-primary`   | Border color         | `rgba(124, 58, 237, 0.3)` |

### Interactive State Variables

| Variable           | Purpose              | Example                        |
| ------------------ | -------------------- | ------------------------------ |
| `--hover-overlay`  | Hover state overlay  | `rgba(124, 58, 237, 0.1)`      |
| `--active-overlay` | Active/pressed state | `rgba(124, 58, 237, 0.2)`      |
| `--focus-ring`     | Focus ring effect    | `0 0 0 2px var(--accent-glow)` |

### FX System Variables

Control visual effects and animations:

| Variable               | Purpose              | Values                                    |
| ---------------------- | -------------------- | ----------------------------------------- |
| `--fx-hover-transform` | Hover transformation | `translateY(-4px) scale(1.02)`            |
| `--fx-hover-glow`      | Hover glow effect    | `0 20px 40px rgba(124, 58, 237, 0.3)`     |
| `--fx-hover-shadow`    | Hover shadow         | `0 8px 32px rgba(124, 58, 237, 0.2)`      |
| `--fx-hover-duration`  | Animation duration   | `400ms`                                   |
| `--fx-hover-easing`    | Animation easing     | `cubic-bezier(0.175, 0.885, 0.32, 1.275)` |
| `--fx-ambient-float`   | Floating animation   | `magical-float 4s ease-in-out infinite`   |
| `--fx-ambient-pulse`   | Pulse animation      | `magical-pulse 2s ease-in-out infinite`   |

### Feature Flags

Enable/disable effects (0 = off, 1 = on):

| Variable                | Purpose                 |
| ----------------------- | ----------------------- |
| `--enable-card-float`   | Card floating animation |
| `--enable-magical-glow` | Magical glow effects    |
| `--enable-sparkles`     | Sparkle effects         |

## 📝 Complete Example

Here's a complete custom theme with light and dark mode support:

```css
/* src/styles/themes/forest.css */

html.scheme-forest {
    /* ========================================
   * FOREST THEME - LIGHT MODE
   * ======================================== */

    /* Primary green palette */
    --accent-primary: #059669;
    --accent-glow: #10b981;
    --accent-secondary: #047857;

    /* Brand colors */
    --brand-gradient-start: #059669;
    --brand-gradient-end: #10b981;
    --brand-gradient: linear-gradient(
        135deg,
        var(--brand-gradient-start),
        var(--brand-gradient-end)
    );

    /* Primary color scale */
    --color-primary-400: #34d399;
    --color-primary-500: #10b981;
    --color-primary-600: #059669;
    --color-primary-700: #047857;

    /* Light mode backgrounds */
    --bg-card: #f0fdf4;
    --border-primary: rgba(5, 150, 105, 0.2);
    --hover-overlay: rgba(5, 150, 105, 0.05);
    --active-overlay: rgba(5, 150, 105, 0.1);

    /* Professional effects */
    --fx-hover-transform: translateY(-2px);
    --fx-hover-shadow: 0 8px 24px rgba(5, 150, 105, 0.2);
    --fx-hover-duration: 300ms;
    --fx-hover-easing: ease-out;

    /* Feature flags */
    --enable-card-float: 0;
    --enable-magical-glow: 0;
    --enable-sparkles: 0;
}

/* ========================================
 * FOREST THEME - DARK MODE
 * ======================================== */

html.theme-dark.scheme-forest,
html.theme-system-dark.scheme-forest {
    /* Dark green backgrounds */
    --bg-primary: #022c22;
    --bg-secondary: #064e3b;
    --bg-tertiary: #065f46;
    --color-background: var(--bg-primary);
    --color-surface: var(--bg-tertiary);

    /* Dark mode text */
    --text-primary: #d1fae5;
    --text-secondary: #86efac;
    --text-tertiary: #6ee7b7;

    /* Enhanced borders and shadows */
    --border-primary: rgba(16, 185, 129, 0.3);
    --hover-overlay: rgba(16, 185, 129, 0.1);

    /* Glowing effects for dark mode */
    --fx-hover-glow: 0 20px 40px rgba(16, 185, 129, 0.3);
    --shadow-lg: 0 10px 15px -3px rgba(0, 0, 0, 0.9), 0 4px 6px -2px rgba(16, 185, 129, 0.3);
}

/* ========================================
 * CUSTOM ANIMATIONS
 * ======================================== */

@keyframes forest-sway {
    0%,
    100% {
        transform: translateX(0px) rotate(0deg);
    }
    33% {
        transform: translateX(-2px) rotate(-0.5deg);
    }
    66% {
        transform: translateX(2px) rotate(0.5deg);
    }
}

html.scheme-forest .floating {
    animation: forest-sway 4s ease-in-out infinite;
}
```

## 🔧 Using Your Custom Theme

### 1. Import the CSS

```svelte
<!-- src/routes/+layout.svelte -->
<script>
    import '../styles/themes/forest.css';
    import { ThemeProvider } from '@goobits/themes/svelte';
    import { themeConfig } from '$lib/config/theme';
</script>

<ThemeProvider config={themeConfig}>
    {@render children()}
</ThemeProvider>
```

### 2. Add to Theme Config

```typescript
// src/lib/config/theme.ts
import type { SchemeConfig } from '@goobits/themes/core';

export const themeConfig = {
    schemes: {
        default: {
            /* ... */
        },
        forest: {
            name: 'forest',
            displayName: 'Forest',
            description: 'Natural green theme inspired by the forest',
            icon: '🌲',
            title: 'Nature Library',
            preview: {
                primary: '#10b981',
                accent: '#059669',
                background: '#f0fdf4',
            },
        },
    } as Record<string, SchemeConfig>,
};
```

### 3. Use the Theme

The theme will be available in your settings and can be selected by users, or applied automatically via route themes.

## 🎯 Best Practices

### 1. Light & Dark Mode Support

Always define both light and dark variants:

```css
/* Light mode - base styles */
html.scheme-mytheme {
    --bg-primary: #ffffff;
}

/* Dark mode - overrides */
html.theme-dark.scheme-mytheme {
    --bg-primary: #0a0a0a;
}
```

### 2. Maintain Accessibility

Ensure sufficient color contrast:

- Text on background: 4.5:1 minimum (WCAG AA)
- Large text: 3:1 minimum
- Interactive elements: Clear focus indicators

### 3. Test Both Themes

Test your color scheme with:

- Light mode base (`theme-light`)
- Dark mode base (`theme-dark`)
- System theme (`theme-system`)

### 4. Respect User Preferences

Use `prefers-reduced-motion` for animations:

```css
@media (prefers-reduced-motion: reduce) {
    html.scheme-mytheme {
        --fx-ambient-float: none;
        --fx-ambient-pulse: none;
    }
}
```

## 🧪 Testing Your Theme

### Visual Inspection

1. Apply the theme in settings
2. Check all pages and components
3. Test light and dark modes
4. Verify interactive states (hover, focus, active)

### Browser DevTools

```javascript
// Check applied CSS variables in console
getComputedStyle(document.documentElement).getPropertyValue('--accent-primary');
```

### Theme Toggle

Use the built-in theme toggle to switch between schemes:

```svelte
<script>
    import { SchemeSelector } from '@goobits/themes/svelte';
</script>

<SchemeSelector />
```

## 📚 Additional Resources

- [Main Package README](../README.md) - Full package documentation
- [Design Tokens Reference](https://github.com/goobits/goobits-themes/blob/main/docs/design-tokens.md) - Complete variable list
- [Color Theory Guide](https://github.com/goobits/goobits-themes/blob/main/docs/color-theory.md) - Choosing colors

## 💡 Tips & Tricks

### Gradient Backgrounds

```css
--brand-gradient: linear-gradient(135deg, #start, #end);
--card-gradient-overlay: linear-gradient(to bottom, transparent 0%, rgba(your-color, 0.2) 100%);
```

### Glow Effects

```css
--fx-hover-glow: 0 20px 40px rgba(your-color, 0.3);
--shadow-lg: 0 10px 15px rgba(0, 0, 0, 0.9), 0 4px 6px rgba(your-color, 0.3);
```

### Color Scales

Generate color scales at [palettte.app](https://palettte.app) or [coolors.co](https://coolors.co).

---

**Need help?** [Open an issue](https://github.com/goobits/goobits-themes/issues) or check the [discussions](https://github.com/goobits/goobits-themes/discussions).
