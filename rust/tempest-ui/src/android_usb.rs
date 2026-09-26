use std::sync::{Mutex, OnceLock};

use jni::{
    JavaVM,
    objects::{GlobalRef, JObject, JObjectArray, JValue},
};

struct AndroidUsb {
    vm: JavaVM,
    activity: GlobalRef,
}

static ANDROID_USB: OnceLock<AndroidUsb> = OnceLock::new();
static USB_CONNECTION: Mutex<Option<GlobalRef>> = Mutex::new(None);

const RTL2832_VENDOR_ID: i32 = 0x0bda;
const RTL2832_PRODUCT_IDS: &[i32] = &[0x2831, 0x2832, 0x2833, 0x2837, 0x2838, 0x2848];

pub fn initialize(app: &slint::android::AndroidApp) -> Result<(), String> {
    if ANDROID_USB.get().is_some() {
        return Ok(());
    }

    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|error| format!("Could not access the Android Java VM: {error}"))?;
    let env = vm
        .attach_current_thread()
        .map_err(|error| format!("Could not attach to the Android Java VM: {error}"))?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let activity = env
        .new_global_ref(activity)
        .map_err(|error| format!("Could not retain the Android activity: {error}"))?;
    drop(env);

    ANDROID_USB
        .set(AndroidUsb { vm, activity })
        .map_err(|_| "Android USB was initialized twice".to_owned())
}

pub fn prepare_rtlsdr() -> Result<i32, String> {
    let android = ANDROID_USB
        .get()
        .ok_or_else(|| "Android USB support did not initialize".to_owned())?;
    let mut env = android
        .vm
        .attach_current_thread()
        .map_err(|error| format!("Could not attach to Android's USB service: {error}"))?;

    let service_name = env
        .new_string("usb")
        .map_err(|error| format!("Could not name Android's USB service: {error}"))?;
    let service_name = JObject::from(service_name);
    let usb_manager = env
        .call_method(
            android.activity.as_obj(),
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("Could not access Android's USB service: {error}"))?;
    if usb_manager.is_null() {
        return Err("Android did not provide a USB service".to_owned());
    }

    let devices = env
        .call_method(&usb_manager, "getDeviceList", "()Ljava/util/HashMap;", &[])
        .and_then(|value| value.l())
        .map_err(|error| format!("Could not enumerate Android USB devices: {error}"))?;
    let values = env
        .call_method(&devices, "values", "()Ljava/util/Collection;", &[])
        .and_then(|value| value.l())
        .map_err(|error| format!("Could not inspect Android USB devices: {error}"))?;
    let array = env
        .call_method(&values, "toArray", "()[Ljava/lang/Object;", &[])
        .and_then(|value| value.l())
        .map(JObjectArray::from)
        .map_err(|error| format!("Could not inspect Android USB devices: {error}"))?;

    let count = env
        .get_array_length(&array)
        .map_err(|error| format!("Could not count Android USB devices: {error}"))?;
    let mut rtl_device = None;
    for index in 0..count {
        let device = env
            .get_object_array_element(&array, index)
            .map_err(|error| format!("Could not inspect USB device {index}: {error}"))?;
        let vendor = env
            .call_method(&device, "getVendorId", "()I", &[])
            .and_then(|value| value.i())
            .map_err(|error| format!("Could not read a USB vendor ID: {error}"))?;
        let product = env
            .call_method(&device, "getProductId", "()I", &[])
            .and_then(|value| value.i())
            .map_err(|error| format!("Could not read a USB product ID: {error}"))?;
        if vendor == RTL2832_VENDOR_ID && RTL2832_PRODUCT_IDS.contains(&product) {
            rtl_device = Some(device);
            break;
        }
    }

    let device = rtl_device.ok_or_else(|| {
        "No supported RTL-SDR was found. Connect it through a USB OTG adapter, then tap Start again."
            .to_owned()
    })?;
    let has_permission = env
        .call_method(
            &usb_manager,
            "hasPermission",
            "(Landroid/hardware/usb/UsbDevice;)Z",
            &[JValue::Object(&device)],
        )
        .and_then(|value| value.z())
        .map_err(|error| format!("Could not check RTL-SDR USB permission: {error}"))?;

    if !has_permission {
        request_permission(&mut env, android.activity.as_obj(), &usb_manager, &device)?;
        return Err(
            "Android USB permission requested. Approve the dialog, then tap Start again."
                .to_owned(),
        );
    }

    let connection = env
        .call_method(
            &usb_manager,
            "openDevice",
            "(Landroid/hardware/usb/UsbDevice;)Landroid/hardware/usb/UsbDeviceConnection;",
            &[JValue::Object(&device)],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("Android could not open the RTL-SDR: {error}"))?;
    if connection.is_null() {
        return Err("Android returned an empty RTL-SDR USB connection".to_owned());
    }

    let interface = env
        .call_method(
            &device,
            "getInterface",
            "(I)Landroid/hardware/usb/UsbInterface;",
            &[JValue::Int(0)],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("Could not access RTL-SDR interface 0: {error}"))?;
    let claimed = env
        .call_method(
            &connection,
            "claimInterface",
            "(Landroid/hardware/usb/UsbInterface;Z)Z",
            &[JValue::Object(&interface), JValue::Bool(1)],
        )
        .and_then(|value| value.z())
        .map_err(|error| format!("Could not claim RTL-SDR interface 0: {error}"))?;
    if !claimed {
        return Err("Android refused to claim RTL-SDR interface 0".to_owned());
    }

    let fd = env
        .call_method(&connection, "getFileDescriptor", "()I", &[])
        .and_then(|value| value.i())
        .map_err(|error| format!("Could not obtain the RTL-SDR USB descriptor: {error}"))?;
    if fd < 0 {
        return Err("Android returned an invalid RTL-SDR USB descriptor".to_owned());
    }

    let connection = env
        .new_global_ref(connection)
        .map_err(|error| format!("Could not retain the RTL-SDR USB connection: {error}"))?;
    *USB_CONNECTION
        .lock()
        .map_err(|_| "The Android USB connection lock was poisoned".to_owned())? = Some(connection);
    Ok(fd)
}

fn request_permission(
    env: &mut jni::JNIEnv<'_>,
    activity: &JObject<'_>,
    usb_manager: &JObject<'_>,
    device: &JObject<'_>,
) -> Result<(), String> {
    let intent = env
        .new_object("android/content/Intent", "()V", &[])
        .map_err(|error| format!("Could not create the USB permission request: {error}"))?;
    let activity_class = env
        .call_method(activity, "getClass", "()Ljava/lang/Class;", &[])
        .and_then(|value| value.l())
        .map_err(|error| format!("Could not address the USB permission response: {error}"))?;
    env.call_method(
        &intent,
        "setClass",
        "(Landroid/content/Context;Ljava/lang/Class;)Landroid/content/Intent;",
        &[JValue::Object(activity), JValue::Object(&activity_class)],
    )
    .map_err(|error| format!("Could not address the USB permission response: {error}"))?;
    let action = env
        .new_string("io.github.stillsilly.tempestsdr.USB_PERMISSION")
        .map(JObject::from)
        .map_err(|error| format!("Could not name the USB permission request: {error}"))?;
    env.call_method(
        &intent,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&action)],
    )
    .map_err(|error| format!("Could not name the USB permission request: {error}"))?;

    // UPDATE_CURRENT | MUTABLE. MUTABLE is required for Android to attach its
    // permission result extras on Android 12 and later.
    let flags = 0x0800_0000 | 0x0200_0000;
    let pending_intent = env
        .call_static_method(
            "android/app/PendingIntent",
            "getActivity",
            "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
            &[
                JValue::Object(activity),
                JValue::Int(0),
                JValue::Object(&intent),
                JValue::Int(flags),
            ],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("Could not create the USB permission callback: {error}"))?;
    env.call_method(
        usb_manager,
        "requestPermission",
        "(Landroid/hardware/usb/UsbDevice;Landroid/app/PendingIntent;)V",
        &[JValue::Object(device), JValue::Object(&pending_intent)],
    )
    .map_err(|error| format!("Could not request RTL-SDR USB permission: {error}"))?;
    Ok(())
}

pub fn release_rtlsdr() {
    let Ok(mut connection) = USB_CONNECTION.lock() else {
        return;
    };
    let Some(connection) = connection.take() else {
        return;
    };
    let Some(android) = ANDROID_USB.get() else {
        return;
    };
    if let Ok(mut env) = android.vm.attach_current_thread() {
        let _ = env.call_method(connection.as_obj(), "close", "()V", &[]);
    }
}
