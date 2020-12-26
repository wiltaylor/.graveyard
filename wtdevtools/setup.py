from setuptools import setup, find_packages

setup(
        name='wtdevtool',
        version='0.1',
        packages = ['wtdevtool'],    #find_packages(),
        include_package_data=True,
        install_requires=['click' ],
        entry_points='''
            [console_scripts]
            sys=wtdevtool.sys:cli
            dev=wtdevtool.dev:cli
        ''',
)
