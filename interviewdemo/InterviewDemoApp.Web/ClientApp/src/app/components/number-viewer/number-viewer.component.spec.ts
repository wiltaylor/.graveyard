import { ComponentFixture, TestBed } from '@angular/core/testing';

import { NumberViewerComponent } from './number-viewer.component';
import {By} from "@angular/platform-browser";
import {DebugElement} from "@angular/core";

describe('NumberViewerComponent', () => {
  let component: NumberViewerComponent;
  let de: DebugElement;
  let fixture: ComponentFixture<NumberViewerComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      declarations: [ NumberViewerComponent ]
    })
    .compileComponents();
  });

  beforeEach(() => {
    fixture = TestBed.createComponent(NumberViewerComponent);
    component = fixture.componentInstance;
    de = fixture.debugElement;
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should have startup controls asking for interval.', () => {
   expect(de.query(By.css("#interval"))).toBeTruthy();
   expect(de.query(By.css("#startButton")).nativeElement.innerText).toBe("Set Interval");
  });

  it('If interval input is not 1 or greater disable Set Interval button.', () => {
    component.interval = -1;
    fixture.detectChanges();

    const btn = de.query(By.css("#startButton")).nativeElement;
    expect(btn.disabled).toBeTrue();
  });

  it('should hide interval input and button after interval button is clicked', () =>{
    const btn = de.query(By.css("#startButton"));
    btn.triggerEventHandler('click', null);

    fixture.detectChanges();

    expect(de.query(By.css("#interval"))).toBeFalsy();
    expect(de.query(By.css("#startButton"))).toBeFalsy();
  });

  it('should show message text area and halt, resume, quit buttons once interval is set.', () => {
    component.interval = 5;
    component.hideIntervalControls = true;

    fixture.detectChanges();

    expect(de.query(By.css("#serverMessages"))).toBeTruthy();
    expect(de.query(By.css("#haltButton"))).toBeTruthy();
    expect(de.query(By.css("#resumeButton"))).toBeTruthy();
    expect(de.query(By.css("#quitButton"))).toBeTruthy();
  });

  it('should hide message text area and main buttons before interval is set.', () => {
    expect(de.query(By.css("#serverMessages"))).toBeFalsy();
    expect(de.query(By.css("#haltButton"))).toBeFalsy();
    expect(de.query(By.css("#resumeButton"))).toBeFalsy();
    expect(de.query(By.css("#quitButton"))).toBeFalsy();
  });

});
