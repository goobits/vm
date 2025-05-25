function deepMerge(base, override) {
  const result = {};
  
  // First, copy all keys from base
  for (const key in base) {
    result[key] = base[key];
  }
  
  // Then apply overrides
  for (const key in override) {
    if (typeof base[key] === 'object' && typeof override[key] === 'object' && \!Array.isArray(base[key])) {
      result[key] = deepMerge(base[key], override[key]);
    } else {
      result[key] = override[key];
    }
  }
  
  return result;
}

const defaultConfig = {
  ports: {
    frontend: 3000,
    backend: 3001,
    postgresql: 3005,
    redis: 3006
  }
};

const userConfig = {
  ports: {
    frontend: 3020,
    backend: 3022
  }
};

const result = deepMerge(defaultConfig, userConfig);
console.log("Merged ports:", JSON.stringify(result.ports, null, 2));
