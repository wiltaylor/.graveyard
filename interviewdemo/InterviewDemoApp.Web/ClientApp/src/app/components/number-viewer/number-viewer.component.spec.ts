import { ComponentFixture, TestBed } from '@angular/core/testing';

import { NumberViewerComponent } from './number-viewer.component';
import {By} from "@angular/platform-browser";
import {DebugElement} from "@angular/core";
import {NumberManagerService} from "../../services/number-manager.service";
import {Observable} from "rxjs";

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

  it('should show number entry, server message text area and halt, resume, quit buttons once interval is set.', () => {
    component.interval = 5;
    component.hideIntervalControls = true;

    fixture.detectChanges();

    expect(de.query(By.css("#numberEntry"))).toBeTruthy();
    expect(de.query(By.css("#numberButton"))).toBeTruthy();
    expect(de.query(By.css("#serverMessages"))).toBeTruthy();
    expect(de.query(By.css("#haltButton"))).toBeTruthy();
    expect(de.query(By.css("#resumeButton"))).toBeTruthy();
    expect(de.query(By.css("#quitButton"))).toBeTruthy();
  });

  it('should hide number entry, server message text area and main buttons before interval is set.', () => {

    expect(de.query(By.css("#numbeEntry"))).toBeFalsy();
    expect(de.query(By.css("#numberButton"))).toBeFalsy();
    expect(de.query(By.css("#serverMessages"))).toBeFalsy();
    expect(de.query(By.css("#haltButton"))).toBeFalsy();
    expect(de.query(By.css("#resumeButton"))).toBeFalsy();
    expect(de.query(By.css("#quitButton"))).toBeFalsy();
  });

  it('should write messages to window when received from the server', () => {
    component.interval = 1;
    component.hideIntervalControls = true;
    let numberService = fixture.debugElement.injector.get(NumberManagerService);

    fixture.detectChanges();

    spyOn(numberService, "connect").and.callFake((interval: number) => {
      return new Observable<void>( o=>{
        o.next();
        o.complete();
      });
    });

    spyOn(numberService, "onMessage").and.callFake(() => {
      return new Observable<string>(o => {
        o.next("Example Message From Server");
        o.complete();
      });
    });

    component.setInterval();
    fixture.detectChanges();

    expect(component.serverText).toContain('Example Message From Server');

  });

  it("should call service when calling add number", () => {
    component.interval = 1;
    component.hideIntervalControls = true;
    component.numberToAdd = 1;

    let numberService = fixture.debugElement.injector.get(NumberManagerService);

    fixture.detectChanges();

    spyOn(numberService, "connect").and.callFake((interval: number) => {
      return new Observable<void>( o=>{
        o.next();
        o.complete();
      });
    });

    let addNumberCalled = false;
    spyOn(numberService, "addNumber").and.callFake((num:number) => {
      return new Observable<void>(o =>{
        o.next();
        o.complete();
        addNumberCalled = true;
      });
    });

    component.addNumber();

    expect(addNumberCalled).toBeTrue();
  });

  it('should be possible to click halt and it call the web socket to halt.', function () {
    component.interval = 1;
    component.hideIntervalControls = true;

    let numberService = fixture.debugElement.injector.get(NumberManagerService);

    fixture.detectChanges();

    spyOn(numberService, "connect").and.callFake((interval: number) => {
      return new Observable<void>( o=>{
        o.next();
        o.complete();
      });
    });

    let haltCalled = false;
    spyOn(numberService, "halt").and.callFake(() => {
      return new Observable<void>(o =>{
        haltCalled = true;
        o.next();
        o.complete();
      });
    });

    component.halt();

    expect(haltCalled).toBeTrue();
  });

  it('should be possible to click resume and it call the web socket to resume.', function () {
    component.interval = 1;
    component.hideIntervalControls = true;

    let numberService = fixture.debugElement.injector.get(NumberManagerService);

    fixture.detectChanges();

    spyOn(numberService, "connect").and.callFake((interval: number) => {
      return new Observable<void>( o=>{
        o.next();
        o.complete();
      });
    });

    let resumeCalled = false;
    spyOn(numberService, "resume").and.callFake(() => {
      return new Observable<void>(o =>{
        resumeCalled = true;
        o.next();
        o.complete();
      });
    });

    component.resume();

    expect(resumeCalled).toBeTrue();
  });


  it('should be possible to click quit and it return the frequencies then drop connection.', function () {
    component.interval = 1;
    component.hideIntervalControls = true;

    let numberService = fixture.debugElement.injector.get(NumberManagerService);

    fixture.detectChanges();

    spyOn(numberService, "connect").and.callFake((interval: number) => {
      return new Observable<void>( o=>{
        o.next();
        o.complete();
      });
    });

    let quitCalled = false;
    spyOn(numberService, "quit").and.callFake(() => {
      return new Observable<void>(o =>{
        quitCalled = true;
        o.next();
        o.complete();
      });
    });

    component.quit();

    expect(quitCalled).toBeTrue();
    expect(component.disableControls).toBeTrue();
  });
});
