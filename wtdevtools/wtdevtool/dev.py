import click
import platform
import os
from click_aliases import ClickAliasedGroup

def parse_repo(slug):
    if slug.startwith('gh'):

@click.group(cls=ClickAliasedGroup)
def cli():
    pass

@cli.command(aliases=['o', 'checkout'])
@click.argument('repo')
def open(repo):
    print('open')

@cli.command(aliases=['new', 'c'])
@click.argument('name')
def create(name):
    print('new')


