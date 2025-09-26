from setuptools import setup, find_packages

setup(
    name="hello-world",
    version="1.0.0",
    description="A simple hello world package for testing",
    author="Test Author",
    author_email="test@example.com",
    url="https://github.com/example/hello-world",
    packages=find_packages(),
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.7",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
    ],
    python_requires=">=3.7",
)